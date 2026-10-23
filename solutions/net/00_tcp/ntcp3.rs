use std::io;
use std::net::SocketAddr;
use tokio::net::TcpListener;

async fn bind_any_port() -> io::Result<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    Ok((listener, addr))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (_listener, addr) = bind_any_port().await?;
    println!("система выдала нам {addr}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn gives_real_loopback_port() {
        let (_l, addr) = timeout(Duration::from_secs(5), bind_any_port())
            .await
            .expect("bind завис")
            .expect("bind на 127.0.0.1:0 должен получиться");
        assert!(addr.ip().is_loopback(), "слушать нужно на 127.0.0.1, а не на {addr}");
        assert_ne!(addr.port(), 0, "вернулся порт 0: это адрес, который просили, а не тот, что выдала система (нужен local_addr)");
    }

    #[tokio::test]
    async fn two_listeners_get_different_ports() {
        let (_a, addr_a) = bind_any_port().await.unwrap();
        let (_b, addr_b) = bind_any_port().await.unwrap();
        assert_ne!(addr_a.port(), addr_b.port(), "два живых слушателя не могут делить порт");
    }

    #[tokio::test]
    async fn address_matches_listener_and_is_busy() {
        let (listener, addr) = bind_any_port().await.unwrap();
        assert_eq!(listener.local_addr().unwrap(), addr, "вернуть нужно адрес именно этого слушателя");
        let err = TcpListener::bind(addr).await.expect_err("второй bind на занятый порт обязан упасть");
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse, "ожидали AddrInUse, получили {err}");
    }
}
