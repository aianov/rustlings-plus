use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;

#[derive(Debug, PartialEq)]
struct Gone;

enum Cmd {
    Add { text: String, ttl: Duration, reply: oneshot::Sender<u32> },
    Count { reply: oneshot::Sender<usize> },
    Expire(u32),
    Stop,
}

#[derive(Clone)]
struct Notes {
    tx: mpsc::Sender<Cmd>,
}

impl Notes {
    async fn add(&self, text: &str, ttl: Duration) -> Result<u32, Gone> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Add { text: text.to_string(), ttl, reply }).await.map_err(|_| Gone)?;
        answer.await.map_err(|_| Gone)
    }

    async fn count(&self) -> Result<usize, Gone> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Count { reply }).await.map_err(|_| Gone)?;
        answer.await.map_err(|_| Gone)
    }

    async fn stop(&self) -> Result<(), Gone> {
        self.tx.send(Cmd::Stop).await.map_err(|_| Gone)
    }
}

fn spawn_notes() -> (Notes, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel(8);
    let myself = tx.downgrade();
    let task = tokio::spawn(async move {
        let mut notes: HashMap<u32, String> = HashMap::new();
        let mut next = 1;
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Add { text, ttl, reply } => {
                    let id = next;
                    next += 1;
                    notes.insert(id, text);
                    let myself = myself.clone();
                    tokio::spawn(async move {
                        sleep(ttl).await;
                        if let Some(tx) = myself.upgrade() {
                            let _ = tx.send(Cmd::Expire(id)).await;
                        }
                    });
                    let _ = reply.send(id);
                }
                Cmd::Count { reply } => {
                    let _ = reply.send(notes.len());
                }
                Cmd::Expire(id) => {
                    notes.remove(&id);
                }
                Cmd::Stop => break,
            }
        }
    });
    (Notes { tx }, task)
}

#[tokio::main]
async fn main() {
    let (notes, task) = spawn_notes();
    notes.add("купить молоко", Duration::from_secs(1)).await.unwrap();
    drop(notes);
    task.await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn notes_expire_after_their_ttl() {
        let (notes, _task) = spawn_notes();
        notes.add("a", Duration::from_millis(100)).await.unwrap();
        notes.add("b", Duration::from_millis(300)).await.unwrap();
        assert_eq!(notes.count().await, Ok(2), "обе заметки только что добавлены");
        sleep(Duration::from_millis(150)).await;
        assert_eq!(notes.count().await, Ok(1), "заметка со сроком 100 мс должна была исчезнуть");
        sleep(Duration::from_millis(200)).await;
        assert_eq!(notes.count().await, Ok(0), "заметка со сроком 300 мс тоже");
    }

    #[tokio::test(start_paused = true)]
    async fn actor_ends_when_the_last_handle_is_dropped() {
        let (notes, task) = spawn_notes();
        let other = notes.clone();
        notes.add("x", Duration::from_secs(60)).await.unwrap();
        drop(notes);
        sleep(Duration::from_millis(10)).await;
        assert!(!task.is_finished(), "ещё жив один Handle, актор работает");
        drop(other);
        let ended = timeout(Duration::from_secs(1), task).await;
        assert!(ended.is_ok(), "все Handle выброшены, а актор не завершился: кто-то внутри держит отправителя и не даёт каналу закрыться");
    }

    #[tokio::test(start_paused = true)]
    async fn calls_after_stop_return_gone() {
        let (notes, task) = spawn_notes();
        assert_eq!(notes.stop().await, Ok(()), "первая остановка проходит");
        task.await.unwrap();
        assert_eq!(notes.add("a", Duration::from_secs(1)).await, Err(Gone), "актор остановлен, add должен вернуть Err(Gone)");
        assert_eq!(notes.count().await, Err(Gone), "актор остановлен, count должен вернуть Err(Gone)");
        assert_eq!(notes.stop().await, Err(Gone), "повторная остановка тоже сообщает Gone");
    }
}
