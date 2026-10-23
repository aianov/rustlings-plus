use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

async fn request_with_retry(server: SocketAddr, msg: &[u8], per_attempt: Duration, attempts: u32) -> io::Result<Vec<u8>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    socket.connect(server).await?;
    let mut buf = vec![0u8; 2048];
    for _ in 0..attempts {
        socket.send(msg).await?;
        if let Ok(n) = timeout(per_attempt, socket.recv(&mut buf)).await {
            return Ok(buf[..n?].to_vec());
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "сервер не ответил ни на одну попытку"))
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
    let reply = request_with_retry(addr, b"ping", Duration::from_millis(200), 3).await?;
    println!("ответ: {}", String::from_utf8_lossy(&reply));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::Instant;

    async fn flaky_server(drop_first: usize) -> (SocketAddr, Arc<AtomicUsize>) {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let intruder = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                let Ok((n, from)) = socket.recv_from(&mut buf).await else { return };
                let nth = counter.fetch_add(1, SeqCst);
                if nth < drop_first {
                    continue;
                }
                let mut reply = b"ok:".to_vec();
                reply.extend_from_slice(&buf[..n]);
                let _ = intruder.send_to(b"evil", from).await;
                let _ = socket.send_to(&reply, from).await;
            }
        });
        (addr, seen)
    }

    #[tokio::test]
    async fn retries_until_answered_and_no_more() {
        let (addr, seen) = flaky_server(2).await;
        let reply = timeout(Duration::from_secs(5), request_with_retry(addr, b"hi", Duration::from_millis(100), 5))
            .await
            .expect("request_with_retry завис")
            .expect("после двух потерь третья попытка должна была получить ответ");
        assert_eq!(reply, b"ok:hi", "принят ответ {:?}: датаграмма от чужого адреса не ответ сервера", String::from_utf8_lossy(&reply));
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(seen.load(SeqCst), 3, "сервер увидел {} запросов вместо 3: повтор шлют только после молчания, по одному, и перестают, получив ответ", seen.load(SeqCst));
    }

    #[tokio::test]
    async fn gives_up_after_attempts_with_timed_out() {
        let (addr, seen) = flaky_server(usize::MAX).await;
        let start = Instant::now();
        let res = timeout(Duration::from_secs(5), request_with_retry(addr, b"hi", Duration::from_millis(100), 3))
            .await
            .expect("request_with_retry не уложилась в 5 секунд: попыток должно быть ограниченное число");
        let err = res.expect_err("сервер молчит, ожидалась ошибка");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "ожидалась TimedOut, пришла {err:?}");
        assert!(start.elapsed() >= Duration::from_millis(280), "вернулись за {:?}: каждая попытка должна честно ждать свои 100 мс, а не уходить все сразу", start.elapsed());
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(seen.load(SeqCst), 3, "сервер увидел {} запросов вместо ровно 3 (attempts)", seen.load(SeqCst));
    }
}
