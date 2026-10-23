use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpStream, lookup_host};
use tokio::time::timeout;

async fn resolve(target: &str) -> io::Result<Vec<SocketAddr>> {
    Ok(lookup_host(target).await?.collect())
}

async fn connect_any(addrs: &[SocketAddr], per_try: Duration) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::InvalidInput, "список адресов пуст");
    for addr in addrs {
        match timeout(per_try, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(e)) => last = e,
            Err(_) => last = io::Error::new(io::ErrorKind::TimedOut, format!("{addr}: не ответил за {per_try:?}")),
        }
    }
    Err(last)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addrs = resolve("localhost:8080").await?;
    println!("{addrs:?}");
    let stream = connect_any(&addrs, Duration::from_secs(1)).await?;
    println!("подключились к {}", stream.peer_addr()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    const SECOND: Duration = Duration::from_secs(1);

    async fn dead_addr() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    }

    #[tokio::test]
    async fn localhost_resolves_with_our_port() {
        let addrs = timeout(Duration::from_secs(5), resolve("localhost:4242")).await.expect("resolve завис").unwrap();
        assert!(!addrs.is_empty(), "у localhost должен быть хотя бы один адрес");
        assert!(addrs.iter().all(|a| a.port() == 4242), "порт из строки должен быть у каждого адреса: {addrs:?}");
        assert!(addrs.iter().any(|a| a.ip().is_loopback()), "localhost должен вести на loopback: {addrs:?}");
    }

    #[tokio::test]
    async fn ip_literal_needs_no_dns() {
        let addrs = resolve("127.0.0.1:80").await.unwrap();
        assert_eq!(addrs, vec!["127.0.0.1:80".parse::<SocketAddr>().unwrap()], "IP с портом возвращается как есть");
    }

    #[tokio::test]
    async fn skips_dead_addresses() {
        let dead = dead_addr().await;
        let live_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let live = live_listener.local_addr().unwrap();
        let stream = timeout(Duration::from_secs(5), connect_any(&[dead, live], SECOND))
            .await
            .expect("завис")
            .expect("первый адрес мёртв, но второй жив: connect_any должна дойти до него");
        assert_eq!(stream.peer_addr().unwrap(), live, "подключились не к живому адресу");
    }

    #[tokio::test]
    async fn reports_the_last_error() {
        let (a, b) = (dead_addr().await, dead_addr().await);
        let err = connect_any(&[a, b], SECOND).await.expect_err("все адреса мертвы, успеха быть не может");
        assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused, "ожидали ошибку последней попытки (Refused), получили {err}");
    }

    #[tokio::test]
    async fn empty_list_is_invalid_input() {
        let err = connect_any(&[], SECOND).await.expect_err("пустой список адресов не может дать соединение");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "для пустого списка ждём InvalidInput, получили {err}");
    }

    #[tokio::test]
    async fn localhost_works_for_an_ipv4_only_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let addrs = resolve(&format!("localhost:{port}")).await.unwrap();
        let stream = timeout(Duration::from_secs(5), connect_any(&addrs, SECOND))
            .await
            .expect("завис")
            .expect("сервер слушает 127.0.0.1, а localhost может начаться с ::1: надо идти дальше по списку");
        assert!(stream.peer_addr().unwrap().ip().is_loopback(), "подключились не к loopback");
    }
}
