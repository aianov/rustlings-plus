
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;
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

pub async fn serve(router: Arc<Router>, mut incoming: mpsc::Receiver<String>, outgoing: mpsc::Sender<Value>) {
    let mut tasks: JoinSet<Value> = JoinSet::new();
    let mut ids: HashMap<tokio::task::Id, Value> = HashMap::new();
    loop {
        tokio::select! {
            frame = incoming.recv() => {
                let Some(text) = frame else { break };
                let router = router.clone();
                let id = frame_id(&text);
                let handle = tasks.spawn(async move { answer(&router, &text).await });
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
    use std::time::Duration;
    use tokio::time::Instant;

    fn router() -> Arc<Router> {
        let mut router = Router::new();
        router.add("feed", "slow", |_| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(json!("лента"))
        });
        router.add("feed", "fast", |_| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(json!("счётчик"))
        });
        router.add("feed", "bomb", |_| async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            panic!("обработчик сломался");
        });
        Arc::new(router)
    }

    fn frame(id: u64, method: &str) -> String {
        json!({"id": id, "service": "feed", "method": method}).to_string()
    }

    async fn run(frames: Vec<String>) -> (Vec<Value>, Duration) {
        let (tx_in, rx_in) = mpsc::channel(100);
        let (tx_out, mut rx_out) = mpsc::channel(100);
        let started = Instant::now();
        let server = tokio::spawn(serve(router(), rx_in, tx_out));
        for frame in frames {
            tx_in.send(frame).await.unwrap();
        }
        drop(tx_in);
        let mut replies = Vec::new();
        while let Some(reply) = rx_out.recv().await {
            replies.push(reply);
        }
        server.await.expect("serve не должен паниковать");
        (replies, started.elapsed())
    }

    #[tokio::test(start_paused = true)]
    async fn fast_request_does_not_wait_for_a_slow_one() {
        let (replies, took) = run(vec![frame(1, "slow"), frame(2, "fast")]).await;
        let ids: Vec<u64> = replies.iter().map(|r| r["id"].as_u64().unwrap()).collect();
        assert_eq!(ids, vec![2, 1], "быстрый ответ пришёл первым, ответы не по порядку запросов: {replies:?}");
        assert_eq!(took, Duration::from_millis(100), "оба запроса вместе заняли 100 мс, а не 110");
    }

    #[tokio::test(start_paused = true)]
    async fn many_requests_run_together() {
        let frames: Vec<String> = (0..50).map(|i| frame(i, "slow")).collect();
        let (replies, took) = run(frames).await;
        assert_eq!(replies.len(), 50, "каждый запрос получил ответ");
        assert_eq!(took, Duration::from_millis(100), "50 запросов по 100 мс шли параллельно");
        let mut ids: Vec<u64> = replies.iter().map(|r| r["id"].as_u64().unwrap()).collect();
        ids.sort();
        assert_eq!(ids, (0..50).collect::<Vec<_>>(), "каждый id ровно один раз");
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_input_does_not_lose_in_flight_replies() {
        let (replies, _) = run(vec![frame(1, "slow")]).await;
        assert_eq!(replies.len(), 1, "вход закрыт сразу после отправки, но запрос в полёте должен получить ответ");
        assert_eq!(replies[0]["ok"], true, "успешный ответ");
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_handler_still_answers_with_its_id() {
        let (replies, _) = run(vec![frame(1, "bomb"), frame(2, "fast")]).await;
        let by_id = |id: u64| replies.iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("нет ответа на запрос {id}: {replies:?}")).clone();
        assert_eq!(by_id(1)["error"]["code"], "internal", "паника в обработчике это ответ internal с тем же id, а не тишина");
        assert_eq!(by_id(2)["ok"], true, "соседний запрос не пострадал");
    }

    #[tokio::test(start_paused = true)]
    async fn bad_frames_get_error_replies_too() {
        let (replies, _) = run(vec!["не json".to_string(), frame(3, "fast")]).await;
        assert_eq!(replies.len(), 2, "оба кадра получили ответ: {replies:?}");
        assert!(replies.iter().any(|r| r["error"]["code"] == "bad_frame"), "битый кадр отмечен");
    }
}
