use async_nats::{Client, Message};
use futures::StreamExt;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct Service {
    pub(crate) stop: watch::Sender<bool>,
    pub(crate) tasks: Vec<JoinHandle<()>>,
    pub(crate) live: Arc<AtomicUsize>,
}

impl Service {
    pub fn new() -> (Service, watch::Receiver<bool>) {
        let (stop, rx) = watch::channel(false);
        (Service { stop, tasks: Vec::new(), live: Arc::new(AtomicUsize::new(0)) }, rx)
    }

    pub fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    pub async fn stop(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

pub(crate) struct Alive(Arc<AtomicUsize>);

impl Alive {
    pub(crate) fn enter(live: &Arc<AtomicUsize>) -> Alive {
        live.fetch_add(1, Ordering::SeqCst);
        Alive(live.clone())
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub async fn sync(client: &Client) {
    let inbox = client.new_inbox();
    let mut sub = client.subscribe(inbox.clone()).await.expect("подписка на inbox");
    client.publish(inbox, "".into()).await.expect("публикация в inbox");
    let _ = tokio::time::timeout(Duration::from_secs(2), sub.next()).await.expect("брокер ответил на круг");
}

pub async fn request_json(client: &Client, subject: &str, body: &Value) -> Result<Value, String> {
    let reply = tokio::time::timeout(Duration::from_secs(2), client.request(subject.to_string(), body.to_string().into()))
        .await
        .map_err(|_| format!("{subject}: нет ответа за две секунды"))?
        .map_err(|e| format!("{subject}: {e}"))?;
    serde_json::from_slice(&reply.payload).map_err(|e| e.to_string())
}

pub(crate) async fn respond<F, Fut>(client: &Client, subject: String, mut stop: watch::Receiver<bool>, live: &Arc<AtomicUsize>, handler: F) -> JoinHandle<()>
where
    F: Fn(Value) -> Fut + Send + 'static,
    Fut: Future<Output = Value> + Send + 'static,
{
    let mut sub = client.subscribe(subject).await.expect("подписка");
    let (client, alive) = (client.clone(), Alive::enter(live));
    tokio::spawn(async move {
        let _alive = alive;
        loop {
            tokio::select! {
                _ = stop.changed() => return,
                message = sub.next() => {
                    let Some(Message { payload, reply, .. }) = message else { return };
                    let request = serde_json::from_slice(&payload).unwrap_or(Value::Null);
                    let answer = handler(request).await;
                    if let Some(reply) = reply {
                        let _ = client.publish(reply, answer.to_string().into()).await;
                    }
                }
            }
        }
    })
}
