use std::io;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use course_support::redis::Client;

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

async fn wait_and_count(addr: &str, channel: &str, counter: &str) -> io::Result<(String, i64)> {
    let stream = TcpStream::connect(addr).await?;
    let mut sub = Sub { stream: BufReader::new(stream) };
    sub.send(&["SUBSCRIBE", channel]).await?;
    let commands = Client::connect_to(addr).await.map_err(|e| io::Error::other(format!("{e:?}")))?;
    loop {
        let frame = sub.read_frame().await?;
        if frame.first().map(String::as_str) == Some("message") && frame.len() == 3 {
            let reply = commands.cmd(&["GET", counter]).await.map_err(|e| io::Error::other(format!("{e:?}")))?;
            return Ok((frame[2].clone(), reply.int().unwrap_or(0)));
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn commands_go_through_another_connection() {
        let s = Scope::new().await;
        let (channel, counter) = (s.key("notify:7"), s.key("user:7:unread"));
        s.cmd(&["SET", &counter, "3"]).await.expect("счётчик");
        let (addr, ch, cnt) = (s.addr(), channel.clone(), counter.clone());
        let waiter = tokio::spawn(async move { wait_and_count(&addr, &ch, &cnt).await });
        for _ in 0..100 {
            if s.cmd(&["PUBSUB", "NUMSUB", &channel]).await.expect("NUMSUB").list().get(1).and_then(|v| v.int()) == Some(1) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        s.cmd(&["PUBLISH", &channel, "новое сообщение"]).await.expect("публикация");
        let got = timeout(Duration::from_secs(3), waiter).await.expect("ответ не пришёл").expect("задача").expect("чтение счётчика");
        assert_eq!(got, ("новое сообщение".to_string(), 3), "сообщение и счётчик");
    }
}
