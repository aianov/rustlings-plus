// Ограниченная очередь защищает шлюз от медленного клиента, но не решает, что с ним делать. Молча выбрасывать сообщения нельзя:
// клиент решит, что чат живой, и будет показывать дыры в переписке. Правильно отключить такого клиента, причём честно,
// с кодом закрытия 1008 (политика): так в логах и на стороне клиента будет видна причина. Закрывающее сообщение тоже идёт
// через ту же очередь, а в очереди уже нет места. TODO: Допиши Conn: ёмкость очереди N сообщений, на N+1-м клиента отключаем.
// Всё, что писатель вынет из очереди, должно закончиться ровно одним Close(1008).

#![allow(dead_code)]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

#[derive(Debug, PartialEq)]
enum Out {
    Text(String),
    Close(u16),
}

#[derive(Debug, PartialEq)]
enum PushError {
    Kicked,
    Gone,
}

struct Conn {
    tx: mpsc::Sender<Out>,
    kicked: Arc<AtomicBool>,
}

impl Conn {
    fn channel(capacity: usize) -> (Conn, mpsc::Receiver<Out>) {
        let (tx, rx) = mpsc::channel(capacity + 1);
        (Conn { tx, kicked: Arc::new(AtomicBool::new(false)) }, rx)
    }

    fn push(&self, text: &str) -> Result<(), PushError> {
        if self.kicked.load(Ordering::Acquire) {
            return Err(PushError::Kicked);
        }
        if self.tx.is_closed() {
            return Err(PushError::Gone);
        }
        if self.tx.capacity() <= 1 {
            if !self.kicked.swap(true, Ordering::AcqRel) {
                let _ = self.tx.try_send(Out::Close(1008));
            }
            return Err(PushError::Kicked);
        }
        self.tx.try_send(Out::Text(text.to_string())).map_err(|_| PushError::Gone)
    }

    fn is_kicked(&self) -> bool {
        self.kicked.load(Ordering::Acquire)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_client_is_kicked_with_close_as_the_last_item() {
        let (conn, mut rx) = Conn::channel(4);
        let mut results = Vec::new();
        for n in 0..10 {
            results.push(conn.push(&format!("m{n}")));
        }
        assert_eq!(&results[..4], &[Ok(()), Ok(()), Ok(()), Ok(())], "первые N сообщений обязаны войти в очередь");
        assert!(results[4..].iter().all(|r| *r == Err(PushError::Kicked)), "начиная с N+1-го клиент отключён: {results:?}");
        assert!(conn.is_kicked());
        drop(conn);
        let mut drained = Vec::new();
        while let Ok(item) = rx.try_recv() {
            drained.push(item);
        }
        let expected: Vec<Out> = (0..4).map(|n| Out::Text(format!("m{n}"))).chain([Out::Close(1008)]).collect();
        assert_eq!(drained, expected, "писатель должен вынуть N сообщений и затем ровно один Close(1008), а не потерять Close из-за полной очереди");
    }

    #[test]
    fn kicked_client_is_not_queued_any_more() {
        let (conn, mut rx) = Conn::channel(2);
        for _ in 0..50 {
            let _ = conn.push("шум");
        }
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 3, "в очереди должно быть 2 сообщения и Close: после отключения ничего не добавляется");
        assert_eq!(conn.push("после"), Err(PushError::Kicked));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn fast_client_is_never_kicked() {
        let (conn, mut rx) = Conn::channel(8);
        let reader = tokio::spawn(async move {
            let mut got = 0usize;
            while let Some(item) = rx.recv().await {
                match item {
                    Out::Text(_) => got += 1,
                    Out::Close(code) => panic!("быстрого клиента отключили кодом {code}"),
                }
            }
            got
        });
        for n in 0..10_000 {
            conn.push(&format!("m{n}")).unwrap_or_else(|e| panic!("клиент, который успевает читать, отключён на сообщении {n}: {e:?}"));
            tokio::task::yield_now().await;
        }
        drop(conn);
        assert_eq!(reader.await.unwrap(), 10_000);
    }

    #[tokio::test]
    async fn slow_client_does_not_hurt_the_fast_one() {
        let (slow, _slow_rx) = Conn::channel(3);
        let (fast, mut fast_rx) = Conn::channel(100);
        for n in 0..50 {
            let _ = slow.push("x");
            fast.push(&format!("f{n}")).unwrap();
        }
        assert!(slow.is_kicked());
        assert!(!fast.is_kicked());
        let mut got = 0;
        while fast_rx.try_recv().is_ok() {
            got += 1;
        }
        assert_eq!(got, 50);
    }

    #[test]
    fn dead_reader_is_reported_as_gone() {
        let (conn, rx) = Conn::channel(4);
        drop(rx);
        assert_eq!(conn.push("в никуда"), Err(PushError::Gone), "читатель очереди исчез: это Gone, а не отключение за медлительность");
        assert!(!conn.is_kicked());
    }
}
