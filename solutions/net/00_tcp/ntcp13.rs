use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

async fn with_timeout<T>(limit: Duration, op: impl Future<Output = io::Result<T>>) -> io::Result<T> {
    match timeout(limit, op).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, format!("операция не уложилась в {limit:?}"))),
    }
}

async fn connect_timeout(addr: SocketAddr, limit: Duration) -> io::Result<TcpStream> {
    with_timeout(limit, TcpStream::connect(addr)).await
}

async fn read_timeout(stream: &mut TcpStream, buf: &mut [u8], limit: Duration) -> io::Result<usize> {
    with_timeout(limit, stream.read(buf)).await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = connect_timeout("127.0.0.1:7".parse().unwrap(), Duration::from_secs(1)).await?;
    let mut buf = [0u8; 16];
    println!("{}", read_timeout(&mut stream, &mut buf, Duration::from_secs(1)).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::pending;
    use std::time::Instant;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test(start_paused = true)]
    async fn pending_future_times_out() {
        let res: io::Result<()> = with_timeout(Duration::from_secs(30), pending()).await;
        let err = res.expect_err("футура никогда не завершится, обёртка обязана вернуть ошибку");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "по истечении времени ждём TimedOut, получили {err}");
    }

    #[tokio::test(start_paused = true)]
    async fn fast_operation_passes_through() {
        let ok = with_timeout(Duration::from_secs(30), async { Ok::<_, io::Error>(42) }).await;
        assert_eq!(ok.unwrap(), 42, "успешный результат должен возвращаться как есть");
        let failed: io::Result<()> =
            with_timeout(Duration::from_secs(30), async { Err(io::Error::from(io::ErrorKind::NotFound)) }).await;
        assert_eq!(
            failed.unwrap_err().kind(),
            io::ErrorKind::NotFound,
            "ошибка самой операции не должна превращаться в TimedOut"
        );
    }

    async fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (client, accepted) = tokio::join!(connect_timeout(addr, Duration::from_secs(2)), listener.accept());
        (client.expect("connect_timeout к живому слушателю должен получиться"), accepted.unwrap().0)
    }

    #[tokio::test]
    async fn silent_peer_gives_timed_out() {
        let (mut client, _silent_server) = pair().await;
        let started = Instant::now();
        let mut buf = [0u8; 8];
        let err = read_timeout(&mut client, &mut buf, Duration::from_millis(150))
            .await
            .expect_err("сервер молчит, чтение не может быть успешным");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "ждали TimedOut, получили {err}");
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(100), "вышли слишком рано ({waited:?}): лимит 150 мс");
        assert!(waited < Duration::from_secs(3), "ждали слишком долго ({waited:?}): лимит 150 мс");
    }

    #[tokio::test]
    async fn data_before_the_deadline_is_read() {
        let (mut client, mut server) = pair().await;
        server.write_all(b"pong").await.unwrap();
        let mut buf = [0u8; 8];
        let n = read_timeout(&mut client, &mut buf, Duration::from_secs(2)).await.expect("данные уже в пути");
        assert_eq!(&buf[..n], b"pong", "прочитано не то, что отправили");
    }

    #[tokio::test]
    async fn refused_stays_refused() {
        let dead = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let err = connect_timeout(dead, Duration::from_secs(2)).await.expect_err("порт никто не слушает");
        assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused, "Refused не должен маскироваться под таймаут, получили {err}");
    }
}
