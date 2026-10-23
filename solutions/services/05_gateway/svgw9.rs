
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

#[derive(Debug, Clone, PartialEq)]
pub struct GwError {
    pub code: &'static str,
    pub message: String,
}

impl GwError {
    pub fn new(code: &'static str, message: &str) -> GwError {
        GwError { code, message: message.to_string() }
    }
}

pub fn ok_reply(id: &Value, data: Value) -> Value {
    json!({"id": id, "ok": true, "data": data})
}

pub fn err_reply(id: &Value, error: &GwError) -> Value {
    json!({"id": id, "ok": false, "error": {"code": error.code, "message": error.message}})
}
pub type Handler = Box<dyn Fn(Value) -> BoxFuture<'static, Result<Value, GwError>> + Send + Sync>;

#[derive(Default)]
pub struct Router {
    table: HashMap<(String, String), Handler>,
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    pub fn add<F, Fut>(&mut self, service: &str, method: &str, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, GwError>> + Send + 'static,
    {
        self.table.insert((service.to_string(), method.to_string()), Box::new(move |data| handler(data).boxed()));
    }

    pub async fn call(&self, service: &str, method: &str, data: Value) -> Result<Value, GwError> {
        match self.table.get(&(service.to_string(), method.to_string())) {
            Some(handler) => handler(data).await,
            None => Err(GwError::new("not_found", &format!("нет метода {service}.{method}"))),
        }
    }
}
pub fn frame_id(text: &str) -> Value {
    serde_json::from_str::<Value>(text).ok().and_then(|f| f.get("id").cloned()).filter(|id| id.is_string() || id.is_number()).unwrap_or(Value::Null)
}

pub async fn answer(router: &Router, text: &str) -> Value {
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        return err_reply(&Value::Null, &GwError::new("bad_frame", "кадр не JSON"));
    };
    let id = frame_id(text);
    let (Some(service), Some(method)) = (frame.get("service").and_then(Value::as_str), frame.get("method").and_then(Value::as_str)) else {
        return err_reply(&id, &GwError::new("invalid_argument", "нужны service и method"));
    };
    match router.call(service, method, frame.get("data").cloned().unwrap_or(Value::Null)).await {
        Ok(data) => ok_reply(&id, data),
        Err(error) => err_reply(&id, &error),
    }
}

pub async fn serve(router: Arc<Router>, limit: usize, mut incoming: mpsc::Receiver<String>, outgoing: mpsc::Sender<Value>) {
    let permits = Arc::new(Semaphore::new(limit));
    let mut tasks: JoinSet<Value> = JoinSet::new();
    let mut ids: HashMap<tokio::task::Id, Value> = HashMap::new();
    loop {
        tokio::select! {
            frame = incoming.recv() => {
                let Some(text) = frame else { break };
                let id = frame_id(&text);
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    let busy = err_reply(&id, &GwError::new("resource_exhausted", "слишком много запросов в работе"));
                    if outgoing.send(busy).await.is_err() {
                        return;
                    }
                    continue;
                };
                let router = router.clone();
                let handle = tasks.spawn(async move {
                    let _permit = permit;
                    answer(&router, &text).await
                });
                ids.insert(handle.id(), id);
            }
            Some(done) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                deliver(&outgoing, &mut ids, done).await;
            }
        }
    }
    while let Some(done) = tasks.join_next_with_id().await {
        deliver(&outgoing, &mut ids, done).await;
    }
}

async fn deliver(outgoing: &mpsc::Sender<Value>, ids: &mut HashMap<tokio::task::Id, Value>, done: Result<(tokio::task::Id, Value), tokio::task::JoinError>) {
    let reply = match done {
        Ok((task, reply)) => {
            ids.remove(&task);
            reply
        }
        Err(failed) => {
            let id = ids.remove(&failed.id()).unwrap_or(Value::Null);
            err_reply(&id, &GwError::new("internal", "обработчик упал"))
        }
    };
    let _ = outgoing.send(reply).await;
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::Instant;

    fn router(peak: Arc<AtomicUsize>) -> Arc<Router> {
        let now = Arc::new(AtomicUsize::new(0));
        let mut router = Router::new();
        router.add("feed", "slow", move |_| {
            let (now, peak) = (now.clone(), peak.clone());
            async move {
                let current = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                now.fetch_sub(1, Ordering::SeqCst);
                Ok(json!("лента"))
            }
        });
        router.add("feed", "bomb", |_| async {
            panic!("обработчик сломался");
        });
        Arc::new(router)
    }

    fn frame(id: u64, method: &str) -> String {
        json!({"id": id, "service": "feed", "method": method}).to_string()
    }

    #[tokio::test(start_paused = true)]
    async fn excess_requests_are_refused_at_once_not_queued() {
        let peak = Arc::new(AtomicUsize::new(0));
        let (tx_in, rx_in) = mpsc::channel(100);
        let (tx_out, mut rx_out) = mpsc::channel(100);
        let started = Instant::now();
        let server = tokio::spawn(serve(router(peak.clone()), 3, rx_in, tx_out));
        for id in 0..10 {
            tx_in.send(frame(id, "slow")).await.unwrap();
        }
        drop(tx_in);
        let mut refused = Vec::new();
        let mut served = Vec::new();
        while let Some(reply) = rx_out.recv().await {
            if reply["ok"] == true {
                served.push(reply["id"].as_u64().unwrap());
            } else {
                assert_eq!(reply["error"]["code"], "resource_exhausted", "отказ по лимиту: {reply}");
                assert_eq!(Instant::now() - started, Duration::ZERO, "отказ приходит сразу, а не после очереди");
                refused.push(reply["id"].as_u64().unwrap());
            }
        }
        server.await.unwrap();
        assert_eq!(served.len(), 3, "обслужены ровно три запроса в полёте: {served:?}");
        assert_eq!(refused.len(), 7, "остальные семь получили отказ с их id: {refused:?}");
        assert_eq!(peak.load(Ordering::SeqCst), 3, "одновременно работало ровно три");
    }

    #[tokio::test(start_paused = true)]
    async fn permit_returns_after_the_request_finishes_and_after_a_panic() {
        let peak = Arc::new(AtomicUsize::new(0));
        let (tx_in, rx_in) = mpsc::channel(100);
        let (tx_out, mut rx_out) = mpsc::channel(100);
        tokio::spawn(serve(router(peak), 1, rx_in, tx_out));
        tx_in.send(frame(1, "bomb")).await.unwrap();
        let first = rx_out.recv().await.unwrap();
        assert_eq!(first["error"]["code"], "internal", "паника это ответ");
        tx_in.send(frame(2, "slow")).await.unwrap();
        let second = rx_out.recv().await.unwrap();
        assert_eq!(second["ok"], true, "лимит 1, но разрешение вернулось после паники");
        tx_in.send(frame(3, "slow")).await.unwrap();
        let third = rx_out.recv().await.unwrap();
        assert_eq!(third["ok"], true, "и после обычного завершения");
    }
}
