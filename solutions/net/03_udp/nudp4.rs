use std::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

async fn call_connected(client: &UdpSocket, server: SocketAddr, msg: &[u8]) -> io::Result<Vec<u8>> {
    client.connect(server).await?;
    client.send(msg).await?;
    let mut buf = vec![0u8; 2048];
    let n = client.recv(&mut buf).await?;
    buf.truncate(n);
    Ok(buf)
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
    let client = UdpSocket::bind("127.0.0.1:0").await?;
    let reply = call_connected(&client, addr, b"ping").await?;
    println!("ответ: {}", String::from_utf8_lossy(&reply));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn stranger_datagram_is_not_an_answer() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        let intruder = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let (_, from) = server.recv_from(&mut buf).await.unwrap();
            intruder.send_to(b"evil", client_addr).await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            server.send_to(b"real", from).await.unwrap();
        });
        let reply = timeout(Duration::from_secs(3), call_connected(&client, server_addr, b"q"))
            .await
            .expect("call_connected завис")
            .expect("call_connected вернул ошибку");
        assert_eq!(reply, b"real", "клиент принял датаграмму от чужого адреса: {:?}", String::from_utf8_lossy(&reply));
    }

    #[tokio::test]
    async fn closed_port_is_reported() {
        let dead = {
            let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            s.local_addr().unwrap()
        };
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let res = timeout(Duration::from_secs(3), call_connected(&client, dead, b"q"))
            .await
            .expect("клиент ждёт ответа от закрытого порта вечно: сокет с connect получает от системы ошибку ConnectionRefused");
        let err = res.expect_err("ответа от мёртвого порта быть не может");
        assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused, "ожидалась ConnectionRefused, пришла {err:?}");
    }
}
