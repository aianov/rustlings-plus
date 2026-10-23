use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Msg {
    Set(String, u32),
    Get(String, oneshot::Sender<Option<u32>>),
    Summary(oneshot::Sender<String>),
}

#[derive(Clone)]
struct Handle {
    tx: mpsc::Sender<Msg>,
}

impl Handle {
    async fn set(&self, key: &str, value: u32) {
        self.tx.send(Msg::Set(key.to_string(), value)).await.unwrap();
    }

    async fn get(&self, key: &str) -> Option<u32> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Msg::Get(key.to_string(), reply)).await.unwrap();
        answer.await.unwrap()
    }

    async fn summary(&self) -> String {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Msg::Summary(reply)).await.unwrap();
        answer.await.unwrap()
    }
}

fn start() -> Handle {
    let (tx, rx) = mpsc::channel(8);
    let handle = Handle { tx };
    tokio::spawn(run(rx, handle.clone()));
    handle
}

async fn run(mut rx: mpsc::Receiver<Msg>, _me: Handle) {
    let mut data: HashMap<String, u32> = HashMap::new();
    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::Set(key, value) => {
                data.insert(key, value);
            }
            Msg::Get(key, reply) => {
                let _ = reply.send(data.get(&key).copied());
            }
            Msg::Summary(reply) => {
                let mut entries: Vec<(&String, &u32)> = data.iter().collect();
                entries.sort();
                let parts: Vec<String> = entries.into_iter().map(|(key, value)| format!("{key}={value}")).collect();
                let _ = reply.send(parts.join(" "));
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let store = start();
    store.set("a", 1).await;
    println!("{}", store.summary().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn get_and_set_work() {
        let store = start();
        store.set("a", 1).await;
        assert_eq!(store.get("a").await, Some(1));
        assert_eq!(store.get("b").await, None, "ключа b нет");
    }

    #[tokio::test(start_paused = true)]
    async fn summary_lists_keys_in_order() {
        let store = start();
        store.set("b", 2).await;
        store.set("a", 1).await;
        store.set("c", 3).await;
        let text = timeout(Duration::from_secs(1), store.summary()).await.expect("summary завис: актор ждёт ответа на вопрос, заданный самому себе");
        assert_eq!(text, "a=1 b=2 c=3", "сводка по возрастанию ключей");
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_summary_is_an_empty_string() {
        let store = start();
        let text = timeout(Duration::from_secs(1), store.summary()).await.expect("summary завис даже на пустом хранилище?");
        assert_eq!(text, "", "ничего нет: сводка пустая строка");
    }

    #[tokio::test(start_paused = true)]
    async fn the_store_keeps_serving_after_a_summary() {
        let store = start();
        store.set("a", 1).await;
        let _ = timeout(Duration::from_secs(1), store.summary()).await;
        store.set("a", 5).await;
        let again = timeout(Duration::from_secs(1), store.get("a")).await.expect("после summary актор перестал отвечать");
        assert_eq!(again, Some(5), "новое значение читается");
    }
}
