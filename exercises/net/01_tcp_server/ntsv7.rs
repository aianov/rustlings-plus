// Когда в сокет пишут многие (ответы, рассылка, пульс), писать напрямую из каждого места
// нельзя: куски разных сообщений перемешаются, а владеть сокетом сможет только один. Решение:
// одна writer-задача владеет записью, остальные шлют ей строки через mpsc-канал.
// TODO: Напиши с нуля spawn_writer. Она запускает задачу, которая берёт строки из канала
// (ограниченного, на 32) и пишет каждую целиком вместе с символом конца строки.
// Когда все отправители закрыты, задача делает shutdown. Если запись упала, задача завершается с этой ошибкой.

use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

fn spawn_writer<W: AsyncWrite + Unpin + Send + 'static>(writer: W) -> (mpsc::Sender<String>, JoinHandle<io::Result<()>>) {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (tx, writer) = spawn_writer(tokio::io::stdout());
    tx.send("привет из writer-задачи".to_owned()).await.ok();
    drop(tx);
    writer.await.map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, BufReader, duplex};
    use tokio::time::timeout;

    #[tokio::test]
    async fn lines_from_many_tasks_do_not_mix() {
        let (w, r) = duplex(64);
        let (tx, handle) = spawn_writer(w);
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(r).lines();
            let mut all = Vec::new();
            while let Some(l) = lines.next_line().await.unwrap() {
                all.push(l);
            }
            all
        });
        let mut senders = Vec::new();
        for i in 0..10 {
            let tx = tx.clone();
            senders.push(tokio::spawn(async move {
                for j in 0..100 {
                    tx.send(format!("task{i}:msg{j}")).await.unwrap();
                }
            }));
        }
        for s in senders {
            s.await.unwrap();
        }
        drop(tx);
        let all = timeout(Duration::from_secs(5), reader).await.expect("читатель не дождался EOF: после закрытия всех отправителей writer должен сделать shutdown").unwrap();
        assert_eq!(all.len(), 1000, "пришло {} строк из 1000", all.len());
        let mut next = [0usize; 10];
        for line in &all {
            let (t, m) = line.split_once(":msg").unwrap_or_else(|| panic!("строка «{line}» испорчена: куски сообщений перемешались"));
            let t: usize = t.strip_prefix("task").and_then(|x| x.parse().ok()).unwrap_or_else(|| panic!("строка «{line}» испорчена"));
            let m: usize = m.parse().unwrap_or_else(|_| panic!("строка «{line}» испорчена"));
            assert_eq!(m, next[t], "сообщения задачи {t} пришли не по порядку");
            next[t] += 1;
        }
        assert!(timeout(Duration::from_secs(3), handle).await.expect("writer-задача не завершилась").unwrap().is_ok(), "при штатном закрытии задача должна вернуть Ok");
    }

    #[tokio::test]
    async fn dead_peer_is_noticed_by_senders() {
        let (w, r) = duplex(16);
        let (tx, handle) = spawn_writer(w);
        drop(r);
        let mut failed = false;
        for _ in 0..1000 {
            if tx.send("hello".to_owned()).await.is_err() {
                failed = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(failed, "writer-задача упала, а отправители всё ещё шлют в никуда: канал не закрылся");
        let res = timeout(Duration::from_secs(3), handle).await.expect("задача не завершилась после ошибки записи").unwrap();
        assert!(res.is_err(), "ошибка записи должна дойти до JoinHandle");
    }
}
