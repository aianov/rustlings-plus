// TcpListener::bind и TcpStream::connect делают всё сразу и не дают настроить сокет. Опции,
// которые действуют до bind и connect (SO_REUSEADDR, backlog, TCP_NODELAY), живут в TcpSocket.
// TODO: Допиши три функции. new_socket создаёт сокет под семейство адреса и ставит опции,
// listen_on делает слушателя с reuseaddr и заданным backlog, connect_nodelay подключается
// с отключённым алгоритмом Nagle.

use std::io;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpSocket, TcpStream};

fn new_socket(addr: SocketAddr, reuse_addr: bool, nodelay: bool) -> io::Result<TcpSocket> {
    todo!()
}

fn listen_on(addr: SocketAddr, backlog: u32) -> io::Result<TcpListener> {
    todo!()
}

async fn connect_nodelay(addr: SocketAddr) -> io::Result<TcpStream> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = listen_on("127.0.0.1:0".parse().unwrap(), 128)?;
    let addr = listener.local_addr()?;
    println!("слушаем {addr}");
    let _client = connect_nodelay(addr).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::timeout;

    fn lo() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    #[tokio::test]
    async fn options_are_applied() {
        let s = new_socket(lo(), true, true).expect("сокет должен создаваться");
        assert!(s.reuseaddr().unwrap(), "reuse_addr = true, а на сокете выключено");
        assert!(s.nodelay().unwrap(), "nodelay = true, а на сокете выключено");
        let s = new_socket(lo(), false, false).unwrap();
        assert!(!s.reuseaddr().unwrap(), "reuse_addr = false должен выключать опцию");
        assert!(!s.nodelay().unwrap(), "nodelay = false должен выключать опцию");
    }

    #[tokio::test]
    async fn listener_accepts() {
        let listener = listen_on(lo(), 64).expect("listen_on должен поднять слушателя");
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback() && addr.port() != 0, "слушатель на {addr}");
        let mut client = TcpStream::connect(addr).await.unwrap();
        let (mut server, _) = timeout(Duration::from_secs(3), listener.accept()).await.expect("accept завис").unwrap();
        client.write_all(b"ok").await.unwrap();
        let mut b = [0u8; 2];
        timeout(Duration::from_secs(3), server.read_exact(&mut b)).await.expect("завис").unwrap();
        assert_eq!(&b, b"ok");
    }

    #[tokio::test]
    async fn address_can_be_reused_after_the_server_closes_first() {
        let first = listen_on(lo(), 16).unwrap();
        let addr = first.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (server_side, _) = first.accept().await.unwrap();
        drop(server_side);
        drop(first);
        drop(client);
        listen_on(addr, 16).expect("тот же адрес сразу после закрытия должен подниматься: нужен SO_REUSEADDR (мешает TIME_WAIT)");
    }

    #[tokio::test]
    async fn connect_sets_nodelay() {
        let listener = listen_on(lo(), 16).unwrap();
        let addr = listener.local_addr().unwrap();
        let plain = TcpStream::connect(addr).await.unwrap();
        assert!(!plain.nodelay().unwrap(), "по умолчанию алгоритм Nagle включён, nodelay выключен");
        let fast = connect_nodelay(addr).await.unwrap();
        assert!(fast.nodelay().unwrap(), "connect_nodelay должна включать TCP_NODELAY");
    }
}
