use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::{Instant, timeout_at};

struct Client {
    sock: UdpSocket,
    next: u32,
}

impl Client {
    async fn connect(server: SocketAddr) -> io::Result<Client> {
        let sock = UdpSocket::bind("127.0.0.1:0").await?;
        sock.connect(server).await?;
        Ok(Client { sock, next: 1 })
    }

    async fn call(&mut self, payload: &[u8], wait: Duration) -> io::Result<Vec<u8>> {
        let seq = self.next;
        self.next = self.next.wrapping_add(1);
        let mut packet = seq.to_be_bytes().to_vec();
        packet.extend_from_slice(payload);
        self.sock.send(&packet).await?;
        let deadline = Instant::now() + wait;
        let mut buf = vec![0u8; 2048];
        loop {
            let n = match timeout_at(deadline, self.sock.recv(&mut buf)).await {
                Ok(read) => read?,
                Err(_) => return Err(io::Error::from(io::ErrorKind::TimedOut)),
            };
            if n >= 4 && buf[..4] == seq.to_be_bytes() {
                return Ok(buf[4..n].to_vec());
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let server = UdpSocket::bind("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    tokio::spawn(async move {
        let mut buf = [0u8; 64];
        if let Ok((n, from)) = server.recv_from(&mut buf).await {
            let _ = server.send_to(&buf[..n], from).await;
        }
    });
    let mut client = Client::connect(addr).await?;
    println!("ответ: {:?}", client.call(b"ping", Duration::from_millis(500)).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::task::JoinHandle;
    use tokio::time::{sleep, timeout};

    #[derive(Clone, Copy, Default)]
    struct Plan {
        delay_ms: u64,
        twice: bool,
        short_junk_first: bool,
    }

    async fn server(plans: Vec<(&'static [u8], Plan)>) -> (SocketAddr, JoinHandle<()>) {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let plans: HashMap<Vec<u8>, Plan> = plans.into_iter().map(|(k, v)| (k.to_vec(), v)).collect();
        let handle = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                let Ok((n, from)) = socket.recv_from(&mut buf).await else { return };
                if n < 4 {
                    continue;
                }
                let packet = buf[..n].to_vec();
                let plan = plans.get(&packet[4..]).copied().unwrap_or_default();
                let socket = socket.clone();
                tokio::spawn(async move {
                    if plan.short_junk_first {
                        let _ = socket.send_to(&[9, 9], from).await;
                    }
                    sleep(Duration::from_millis(plan.delay_ms)).await;
                    let _ = socket.send_to(&packet, from).await;
                    if plan.twice {
                        sleep(Duration::from_millis(40)).await;
                        let _ = socket.send_to(&packet, from).await;
                    }
                });
            }
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn late_reply_is_not_taken_for_the_next_call() {
        let (addr, srv) = server(vec![(b"a", Plan { delay_ms: 200, ..Plan::default() }), (b"b", Plan { delay_ms: 250, ..Plan::default() })]).await;
        let mut c = Client::connect(addr).await.unwrap();
        let first = c.call(b"a", Duration::from_millis(100)).await;
        assert_eq!(first.expect_err("на 'a' сервер отвечает через 200 мс, ждали 100: ожидался таймаут").kind(), io::ErrorKind::TimedOut);
        let second = timeout(Duration::from_secs(3), c.call(b"b", Duration::from_millis(800))).await.expect("call завис").expect("на 'b' ответ приходит через 250 мс, ждали 800");
        assert_eq!(second, b"b", "на вопрос 'b' клиент принял ответ на другой вопрос: {:?}", String::from_utf8_lossy(&second));
        srv.abort();
    }

    #[tokio::test]
    async fn duplicate_reply_is_not_taken_for_the_next_call() {
        let (addr, srv) = server(vec![(b"one", Plan { twice: true, ..Plan::default() }), (b"two", Plan { delay_ms: 150, ..Plan::default() })]).await;
        let mut c = Client::connect(addr).await.unwrap();
        let one = timeout(Duration::from_secs(3), c.call(b"one", Duration::from_millis(500))).await.expect("завис").unwrap();
        assert_eq!(one, b"one");
        let two = timeout(Duration::from_secs(3), c.call(b"two", Duration::from_millis(800))).await.expect("завис").unwrap();
        assert_eq!(two, b"two", "повторная копия прошлого ответа сошла за ответ на новый вопрос: {:?}", String::from_utf8_lossy(&two));
        srv.abort();
    }

    #[tokio::test]
    async fn short_junk_does_not_panic_or_stop_the_call() {
        let (addr, srv) = server(vec![(b"q", Plan { short_junk_first: true, delay_ms: 30, ..Plan::default() })]).await;
        let mut c = Client::connect(addr).await.unwrap();
        let reply = timeout(Duration::from_secs(3), c.call(b"q", Duration::from_millis(500))).await.expect("завис").expect("короткий мусор не должен ломать вызов");
        assert_eq!(reply, b"q");
        srv.abort();
    }

    #[tokio::test]
    async fn junk_flood_does_not_extend_the_deadline() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let flood = tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let Ok((_, from)) = socket.recv_from(&mut buf).await else { return };
            loop {
                let _ = socket.send_to(&[1], from).await;
                let _ = socket.send_to(&[0, 0, 3, 232, b'x'], from).await;
                sleep(Duration::from_millis(30)).await;
            }
        });
        let mut c = Client::connect(addr).await.unwrap();
        let start = std::time::Instant::now();
        let res = timeout(Duration::from_secs(3), c.call(b"q", Duration::from_millis(250)))
            .await
            .expect("call не вернулась за 3 секунды: каждый выброшенный чужой пакет заново запускает ожидание, а срок должен быть один на весь вызов");
        assert_eq!(res.expect_err("настоящего ответа не будет").kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_millis(1000), "ожидание затянулось до {:?}: срок 250 мс считается от начала вызова", start.elapsed());
        flood.abort();
    }
}
