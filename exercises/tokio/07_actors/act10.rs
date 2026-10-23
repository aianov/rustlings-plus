// По одному соединению к внешнему сервису идут запросы от многих задач сразу, а ответы приходят
// вперемешку: каждый несёт номер запроса. Нужна прослойка, у которой `call` отдаёт ответ именно на
// твой запрос, как бы ни переставил их сервер. Запросы не должны ждать друг друга. Если соединение
// закрылось, все ждущие получают `None`, а не висят вечно.
// TODO: Допиши `Mux::new` и `Mux::call`.
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

impl Mux {
    fn new(wire: mpsc::Sender<Request>, replies: mpsc::Receiver<Response>) -> Mux {
        todo!()
    }

    async fn call(&self, payload: &str) -> Option<String> {
        todo!()
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
    println!("{:?}", mux.call("привет").await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{Instant, sleep, timeout};

    fn fake_server() -> (mpsc::Sender<Request>, mpsc::Receiver<Response>, Arc<Mutex<Vec<u64>>>) {
        let (wire_tx, mut wire_rx) = mpsc::channel::<Request>(64);
        let (reply_tx, reply_rx) = mpsc::channel::<Response>(64);
        let ids = Arc::new(Mutex::new(Vec::new()));
        let seen = ids.clone();
        tokio::spawn(async move {
            while let Some((id, payload)) = wire_rx.recv().await {
                seen.lock().unwrap().push(id);
                let reply_tx = reply_tx.clone();
                tokio::spawn(async move {
                    let delay: u64 = payload.rsplit(':').next().and_then(|d| d.parse().ok()).unwrap_or(0);
                    sleep(Duration::from_millis(delay)).await;
                    let _ = reply_tx.send((id, format!("ok {payload}"))).await;
                });
            }
        });
        (wire_tx, reply_rx, ids)
    }

    #[tokio::test(start_paused = true)]
    async fn every_caller_gets_its_own_reply_even_when_the_server_reorders() {
        let (wire, replies, _) = fake_server();
        let mux = Mux::new(wire, replies);
        let start = Instant::now();
        let (a, b, c) = tokio::join!(mux.call("a:300"), mux.call("b:100"), mux.call("c:200"));
        assert_eq!(a.as_deref(), Some("ok a:300"), "ответ на запрос a достался не тому, кто его задал");
        assert_eq!(b.as_deref(), Some("ok b:100"), "ответ на запрос b достался не тому, кто его задал");
        assert_eq!(c.as_deref(), Some("ok c:200"), "ответ на запрос c достался не тому, кто его задал");
        assert_eq!(start.elapsed(), Duration::from_millis(300), "запросы идут одновременно: ждать надо самый долгий, 300 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn request_ids_are_unique() {
        let (wire, replies, ids) = fake_server();
        let mux = Arc::new(Mux::new(wire, replies));
        let mut tasks = Vec::new();
        for n in 0..50 {
            let mux = mux.clone();
            tasks.push(tokio::spawn(async move { mux.call(&format!("r{n}:10")).await }));
        }
        for (n, task) in tasks.into_iter().enumerate() {
            assert_eq!(task.await.unwrap(), Some(format!("ok r{n}:10")), "запрос {n} получил чужой ответ");
        }
        let mut ids = ids.lock().unwrap().clone();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 50, "пятьдесят запросов должны нести пятьдесят разных номеров");
    }

    #[tokio::test(start_paused = true)]
    async fn closed_connection_releases_waiting_callers() {
        let (wire_tx, _wire_rx) = mpsc::channel::<Request>(8);
        let (reply_tx, reply_rx) = mpsc::channel::<Response>(8);
        let mux = Arc::new(Mux::new(wire_tx, reply_rx));
        let waiting = {
            let mux = mux.clone();
            tokio::spawn(async move { mux.call("x").await })
        };
        sleep(Duration::from_millis(10)).await;
        drop(reply_tx);
        let outcome = timeout(Duration::from_secs(1), waiting).await.expect("соединение закрыто, а ждущий вызов так и висит");
        assert_eq!(outcome.unwrap(), None, "ждущий вызов при закрытии соединения получает None");
    }

    #[tokio::test(start_paused = true)]
    async fn call_on_a_dead_wire_returns_none() {
        let (wire_tx, wire_rx) = mpsc::channel::<Request>(8);
        let (_reply_tx, reply_rx) = mpsc::channel::<Response>(8);
        drop(wire_rx);
        let mux = Mux::new(wire_tx, reply_rx);
        let outcome = timeout(Duration::from_secs(1), mux.call("x")).await.expect("запрос в закрытую линию не должен зависать");
        assert_eq!(outcome, None, "отправить некуда: None");
    }
}
