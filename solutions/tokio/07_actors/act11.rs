use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

type Request = (u64, String);
type Response = (u64, String);
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<String>>>>;

struct Mux {
    wire: mpsc::Sender<Request>,
    pending: Pending,
    next_id: AtomicU64,
}

struct Registration {
    pending: Pending,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.pending.lock().unwrap().remove(&self.id);
    }
}

impl Mux {
    fn new(wire: mpsc::Sender<Request>, mut replies: mpsc::Receiver<Response>) -> Mux {
        let pending: Pending = Arc::default();
        let table = pending.clone();
        tokio::spawn(async move {
            while let Some((id, reply)) = replies.recv().await {
                let waiter = table.lock().unwrap().remove(&id);
                if let Some(waiter) = waiter {
                    let _ = waiter.send(reply);
                }
            }
            table.lock().unwrap().clear();
        });
        Mux { wire, pending, next_id: AtomicU64::new(1) }
    }

    fn pending_len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    async fn call(&self, payload: &str) -> Option<String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let _registration = Registration { pending: self.pending.clone(), id };
        self.wire.send((id, payload.to_string())).await.ok()?;
        rx.await.ok()
    }
}

#[tokio::main]
async fn main() {
    let (wire_tx, mut wire_rx) = mpsc::channel::<Request>(8);
    let (reply_tx, reply_rx) = mpsc::channel::<Response>(8);
    tokio::spawn(async move {
        while let Some((id, payload)) = wire_rx.recv().await {
            reply_tx.send((id, payload.to_uppercase())).await.unwrap();
        }
    });
    let mux = Mux::new(wire_tx, reply_rx);
    println!("{:?} {}", mux.call("привет").await, mux.pending_len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    fn fake_server() -> (mpsc::Sender<Request>, mpsc::Receiver<Response>) {
        let (wire_tx, mut wire_rx) = mpsc::channel::<Request>(64);
        let (reply_tx, reply_rx) = mpsc::channel::<Response>(64);
        tokio::spawn(async move {
            while let Some((id, payload)) = wire_rx.recv().await {
                let reply_tx = reply_tx.clone();
                tokio::spawn(async move {
                    let Some(delay) = payload.rsplit(':').next().and_then(|d| d.parse::<u64>().ok()) else {
                        return;
                    };
                    sleep(Duration::from_millis(delay)).await;
                    let _ = reply_tx.send((id, format!("ok {payload}"))).await;
                });
            }
        });
        (wire_tx, reply_rx)
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_calls_leave_no_trace() {
        let (wire, replies) = fake_server();
        let mux = Mux::new(wire, replies);
        for n in 0..5 {
            let outcome = timeout(Duration::from_millis(50), mux.call(&format!("never{n}:silent"))).await;
            assert!(outcome.is_err(), "сервер не отвечает, вызов должен упереться в таймаут");
        }
        assert_eq!(mux.pending_len(), 0, "пять вызовов отменены, а в таблице ожидания остались записи: утечка");
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_reply_is_ignored_and_the_line_keeps_working() {
        let (wire, replies) = fake_server();
        let mux = Mux::new(wire, replies);
        let outcome = timeout(Duration::from_millis(50), mux.call("slow:200")).await;
        assert!(outcome.is_err(), "ответ придёт через 200 мс, ждать согласны 50");
        sleep(Duration::from_millis(300)).await;
        assert_eq!(mux.pending_len(), 0, "после опоздавшего ответа таблица пуста");
        assert_eq!(mux.call("fast:10").await.as_deref(), Some("ok fast:10"), "после отмены линия должна работать как раньше");
        assert_eq!(mux.pending_len(), 0, "успешный вызов не оставляет записи");
    }

    #[tokio::test(start_paused = true)]
    async fn aborting_the_calling_task_cleans_up_too() {
        let (wire, replies) = fake_server();
        let mux = Arc::new(Mux::new(wire, replies));
        let caller = {
            let mux = mux.clone();
            tokio::spawn(async move { mux.call("never:silent").await })
        };
        sleep(Duration::from_millis(10)).await;
        assert_eq!(mux.pending_len(), 1, "пока вызов ждёт, запись в таблице есть");
        caller.abort();
        let _ = caller.await;
        assert_eq!(mux.pending_len(), 0, "задача отменена в ожидании: запись должна исчезнуть вместе с вызовом");
    }
}
