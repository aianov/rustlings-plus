// Уведомления в реальном времени через pub/sub: сервис подписывается на канал, а другие публикуют в него.
// Соединение после SUBSCRIBE переходит в особый режим: сервер сам присылает сообщения массивами
// `["message", канал, данные]`, а первым приходит подтверждение `["subscribe", канал, число_подписок]`.
// Чтение кадров уже готово (`Sub::read_frame`).
// TODO: `subscribe` открывает соединение и подписывается, `next_message` возвращает данные следующего
// сообщения канала, пропуская служебные кадры.

use std::io;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

struct Sub {
    stream: BufReader<TcpStream>,
}

impl Sub {
    async fn send(&mut self, args: &[&str]) -> io::Result<()> {
        let mut out = format!("*{}\r\n", args.len());
        for a in args {
            out.push_str(&format!("${}\r\n{a}\r\n", a.len()));
        }
        self.stream.get_mut().write_all(out.as_bytes()).await
    }

    async fn line(&mut self) -> io::Result<String> {
        let mut line = String::new();
        if self.stream.read_line(&mut line).await? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение"));
        }
        Ok(line.trim_end_matches("\r\n").to_string())
    }

    async fn read_frame(&mut self) -> io::Result<Vec<String>> {
        let head = self.line().await?;
        let parse = |s: &str| s.parse::<usize>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("плохой заголовок {s}")));
        match head.split_at(head.len().min(1)) {
            ("*", n) | (">", n) => {
                let mut items = Vec::new();
                for _ in 0..parse(n)? {
                    let item = self.line().await?;
                    if let Some(len) = item.strip_prefix('$') {
                        let mut data = vec![0u8; parse(len)? + 2];
                        self.stream.read_exact(&mut data).await?;
                        data.truncate(data.len() - 2);
                        items.push(String::from_utf8_lossy(&data).into_owned());
                    } else {
                        items.push(item.get(1..).unwrap_or_default().to_string());
                    }
                }
                Ok(items)
            }
            ("-", e) => Err(io::Error::other(e.to_string())),
            (_, rest) => Ok(vec![rest.to_string()]),
        }
    }
}

async fn subscribe(addr: &str, channel: &str) -> io::Result<Sub> {
    let stream = TcpStream::connect(addr).await?;
    let _ = channel;
    Ok(Sub { stream: BufReader::new(stream) })
}

async fn next_message(sub: &mut Sub) -> io::Result<String> {
    let frame = sub.read_frame().await?;
    Ok(frame.last().cloned().unwrap_or_default())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn receives_published_messages() {
        let s = Scope::new().await;
        let channel = s.key("notify:7");
        let mut sub = subscribe(&s.addr(), &channel).await.expect("подписка");
        for _ in 0..50 {
            if s.cmd(&["PUBSUB", "NUMSUB", &channel]).await.expect("NUMSUB").list().get(1).and_then(|v| v.int()) == Some(1) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        for text in ["лайк от ann", "подписка bob", "комментарий kate"] {
            s.cmd(&["PUBLISH", &channel, text]).await.expect("публикация");
        }
        s.cmd(&["PUBLISH", &s.key("notify:8"), "чужой канал"]).await.expect("публикация");
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(timeout(Duration::from_secs(3), next_message(&mut sub)).await.expect("сообщение не пришло").expect("чтение"));
        }
        assert_eq!(got, ["лайк от ann", "подписка bob", "комментарий kate"], "только данные сообщений канала, по порядку, без служебных кадров");
    }
}
