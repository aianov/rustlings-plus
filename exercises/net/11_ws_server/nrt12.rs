// Кроме ответов на запросы, шлюз сам шлёт клиенту события: «тебе поставили лайк», «пришло сообщение». Они бегут по тому же
// соединению, вперемешку с ответами, и клиент должен их различать: у ответа есть id, у события есть имя. Ещё клиенту нужно знать,
// что он ничего не пропустил, поэтому события нумеруются подряд внутри соединения (seq с единицы). Если очередь переполнилась и
// событие пропало, в нумерации должна остаться дыра, а порядок номеров в очереди обязан совпадать с порядком отправки.
// TODO: Допиши Connection: reply и push (вид сообщений смотри в тестах).

use serde_json::{Value, json};
use std::sync::Mutex;
use tokio::sync::mpsc;

struct Connection {
    tx: mpsc::Sender<String>,
    seq: Mutex<u64>,
}

impl Connection {
    fn new(capacity: usize) -> (Connection, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Connection { tx, seq: Mutex::new(0) }, rx)
    }

    fn reply(&self, id: u64, data: Value) -> bool {
        let _ = (id, data);
        todo!()
    }

    fn push(&self, event: &str, data: Value) -> bool {
        let _ = (event, data);
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut mpsc::Receiver<String>) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(text) = rx.try_recv() {
            out.push(serde_json::from_str(&text).expect("в очередь попал не JSON"));
        }
        out
    }

    #[test]
    fn replies_and_pushes_look_different() {
        let (conn, mut rx) = Connection::new(16);
        assert!(conn.reply(3, json!({"liked": true})));
        assert!(conn.push("like.received", json!({"post": 42, "from": "alice"})));
        let items = drain(&mut rx);
        assert_eq!(items[0], json!({"id": 3, "ok": true, "data": {"liked": true}}));
        assert_eq!(items[1], json!({"event": "like.received", "data": {"post": 42, "from": "alice"}, "seq": 1}));
        assert!(items[1].get("id").is_none(), "у события нет id: клиент не должен принять его за ответ");
        assert!(items[0].get("event").is_none());
    }

    #[test]
    fn seq_counts_only_pushes() {
        let (conn, mut rx) = Connection::new(64);
        conn.push("a", Value::Null);
        conn.reply(1, Value::Null);
        conn.reply(2, Value::Null);
        conn.push("b", Value::Null);
        let seqs: Vec<u64> = drain(&mut rx).iter().filter_map(|m| m["seq"].as_u64()).collect();
        assert_eq!(seqs, vec![1, 2], "ответы не расходуют номера событий");
    }

    #[test]
    fn dropped_event_leaves_a_gap_in_numbering() {
        let (conn, mut rx) = Connection::new(2);
        let results: Vec<bool> = (0..5).map(|n| conn.push("tick", json!(n))).collect();
        assert_eq!(results, vec![true, true, false, false, false], "очередь на 2 места: влезли два события");
        assert_eq!(drain(&mut rx).len(), 2);
        assert!(conn.push("tick", json!(5)));
        let after = drain(&mut rx);
        assert_eq!(after[0]["seq"], json!(6), "пропавшие события 3, 4 и 5 расходуют номера, чтобы клиент увидел дыру");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_pushes_arrive_in_seq_order() {
        let (conn, mut rx) = Connection::new(8192);
        let conn = std::sync::Arc::new(conn);
        let mut tasks = Vec::new();
        for worker in 0..4 {
            let conn = conn.clone();
            tasks.push(tokio::spawn(async move {
                for n in 0..500 {
                    assert!(conn.push("tick", json!({"worker": worker, "n": n})));
                    if n % 5 == 0 {
                        assert!(conn.reply(n, Value::Null));
                    }
                }
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        let items = drain(&mut rx);
        let seqs: Vec<u64> = items.iter().filter_map(|m| m["seq"].as_u64()).collect();
        assert_eq!(seqs.len(), 2000);
        let expected: Vec<u64> = (1..=2000).collect();
        assert_eq!(seqs, expected, "номера в очереди идут не подряд: номер выдан в одном месте, а сообщение положено в очередь в другом, и между ними вклинился второй поток");
        assert_eq!(items.len() - seqs.len(), 400, "ответы не должны теряться среди событий");
    }
}
