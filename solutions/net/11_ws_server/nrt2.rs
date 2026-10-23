// В шлюзе пишут в одного клиента сразу несколько мест: ответ на его запрос, рассылка из комнаты, уведомление.
// Если каждое место лезет в сокет само, они ждут друг друга, а один медленный клиент подвешивает всех, кто ему пишет.
// TODO: Отдай сокет клиента одной задаче-писателю, а остальным выдай очередь с ограниченной ёмкостью. Писатель отправляет
// сообщения строками (текст и `\n`), по порядку, и аккуратно заканчивает, когда писать больше некому.

#![allow(dead_code)]
use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct Outbox {
    tx: mpsc::Sender<String>,
}

#[derive(Debug, PartialEq)]
enum PushError {
    Full,
    Closed,
}

impl Outbox {
    fn try_push(&self, message: String) -> Result<(), PushError> {
        self.tx.try_send(message).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => PushError::Full,
            mpsc::error::TrySendError::Closed(_) => PushError::Closed,
        })
    }

    async fn push(&self, message: String) -> Result<(), PushError> {
        self.tx.send(message).await.map_err(|_| PushError::Closed)
    }
}

fn spawn_writer<W>(writer: W, capacity: usize) -> (Outbox, JoinHandle<io::Result<()>>)
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<String>(capacity);
    let task = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(message) = rx.recv().await {
            let mut line = message.into_bytes();
            line.push(b'\n');
            writer.write_all(&line).await?;
            writer.flush().await?;
        }
        writer.shutdown().await
    });
    (Outbox { tx }, task)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, duplex};
    use tokio::time::timeout;

    async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
        match timeout(Duration::from_secs(5), fut).await {
            Ok(v) => v,
            Err(_) => panic!("{what}: за 5 секунд ничего не произошло"),
        }
    }

    #[tokio::test]
    async fn producers_keep_order_and_lines_stay_whole() {
        let (server_side, mut client_side) = duplex(64);
        let (outbox, task) = spawn_writer(server_side, 16);
        let mut producers = Vec::new();
        for p in 0..3 {
            let outbox = outbox.clone();
            producers.push(tokio::spawn(async move {
                for n in 0..100 {
                    outbox.push(format!("p{p}-{n}")).await.unwrap();
                }
            }));
        }
        drop(outbox);
        let mut text = String::new();
        within("чтение клиента", client_side.read_to_string(&mut text)).await.unwrap();
        for p in producers {
            p.await.unwrap();
        }
        within("завершение писателя", task).await.unwrap().unwrap();
        let mut next: HashMap<u32, u32> = HashMap::new();
        let mut total = 0;
        for line in text.lines() {
            let (p, n) = line.strip_prefix('p').and_then(|r| r.split_once('-')).expect("строка склеилась или порвалась");
            let (p, n): (u32, u32) = (p.parse().unwrap(), n.parse().unwrap());
            let expected = next.entry(p).or_insert(0);
            assert_eq!(n, *expected, "сообщения одного отправителя пришли не по порядку или потерялись");
            *expected += 1;
            total += 1;
        }
        assert_eq!(total, 300, "должно прийти ровно 300 сообщений");
        assert!(text.ends_with('\n'), "каждая строка заканчивается переводом строки");
    }

    #[tokio::test]
    async fn writer_finishes_and_closes_the_socket_when_nobody_can_write() {
        let (server_side, mut client_side) = duplex(1024);
        let (outbox, task) = spawn_writer(server_side, 4);
        outbox.push("последнее".to_string()).await.unwrap();
        drop(outbox);
        let mut got = Vec::new();
        within("клиент должен увидеть конец потока", client_side.read_to_end(&mut got)).await.unwrap();
        assert_eq!(String::from_utf8(got).unwrap(), "последнее\n");
        within("писатель", task).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn queue_is_bounded_when_the_client_does_not_read() {
        let (server_side, _client_side) = duplex(8);
        let (outbox, _task) = spawn_writer(server_side, 4);
        let mut accepted = 0;
        let mut saw_full = false;
        for n in 0..200 {
            match outbox.try_push(format!("сообщение номер {n}")) {
                Ok(()) => accepted += 1,
                Err(PushError::Full) => {
                    saw_full = true;
                    break;
                }
                Err(PushError::Closed) => panic!("писатель не должен закрываться, пока клиент жив"),
            }
            tokio::task::yield_now().await;
        }
        assert!(saw_full, "клиент ничего не читает, а очередь приняла {accepted} сообщений и не заполнилась: она должна быть ограничена");
        assert!(accepted <= 6, "очередь на 4 места приняла {accepted}: ёмкость не соблюдается");
    }

    #[tokio::test]
    async fn dead_client_ends_the_writer_and_closes_the_outbox() {
        let (server_side, client_side) = duplex(64);
        let (outbox, task) = spawn_writer(server_side, 4);
        drop(client_side);
        let _ = outbox.push("в пустоту".to_string()).await;
        let result = within("писатель должен закончиться сам", task).await.unwrap();
        assert!(result.is_err(), "запись в закрытое соединение это ошибка, и писатель должен её вернуть");
        assert_eq!(outbox.push("ещё".to_string()).await, Err(PushError::Closed), "очередь мёртвого писателя должна сообщать Closed");
        assert_eq!(outbox.try_push("ещё".to_string()), Err(PushError::Closed));
    }
}
