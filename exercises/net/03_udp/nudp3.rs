// Приёмник читает датаграммы в буфер на 512 байт. Сообщения по 1000 байт приходят «целыми», но
// на самом деле у них молча отрезан хвост, и дальше по системе гуляют битые данные. Ядро не
// скажет, что что-то отрезано.
// TODO: Почини recv_limited: сообщение длиннее limit байт должно давать ошибку InvalidData, а не
// обрезанный результат. Сообщение ровно в limit байт нормально. Следующие датаграммы читаются как обычно.

use std::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

async fn recv_limited(socket: &UdpSocket, limit: usize) -> io::Result<(Vec<u8>, SocketAddr)> {
    let mut buf = vec![0u8; 512];
    let (n, from) = socket.recv_from(&mut buf).await?;
    Ok((buf[..n].to_vec(), from))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let addr = socket.local_addr()?;
    let sender = UdpSocket::bind("127.0.0.1:0").await?;
    sender.send_to(&[7u8; 1500], addr).await?;
    match recv_limited(&socket, 1000).await {
        Ok((data, _)) => println!("принято {} байт", data.len()),
        Err(e) => println!("отказ: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn read(socket: &UdpSocket, limit: usize) -> io::Result<Vec<u8>> {
        timeout(Duration::from_secs(3), recv_limited(socket, limit))
            .await
            .expect("recv_limited не дождалась датаграммы")
            .map(|(data, _)| data)
    }

    async fn setup() -> (UdpSocket, UdpSocket) {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.connect(receiver.local_addr().unwrap()).await.unwrap();
        (receiver, sender)
    }

    #[tokio::test]
    async fn message_of_exactly_limit_is_fine() {
        let (rx, tx) = setup().await;
        let msg: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        tx.send(&msg).await.unwrap();
        let got = read(&rx, 1000).await.expect("сообщение ровно в limit байт должно приниматься");
        assert_eq!(got.len(), 1000, "принято {} байт вместо 1000", got.len());
        assert_eq!(got, msg, "содержимое отличается от отправленного");
    }

    #[tokio::test]
    async fn one_byte_too_long_is_an_error() {
        let (rx, tx) = setup().await;
        tx.send(&[1u8; 1001]).await.unwrap();
        let err = read(&rx, 1000).await.expect_err("датаграмма на 1001 байт при limit 1000 принята как обычная: хвост потерян молча, а должна быть ошибка");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "ошибка должна быть InvalidData, пришла {err:?}");
    }

    #[tokio::test]
    async fn oversized_does_not_poison_the_next_read() {
        let (rx, tx) = setup().await;
        tx.send(&[2u8; 3000]).await.unwrap();
        tx.send(b"tail").await.unwrap();
        assert!(read(&rx, 1000).await.is_err(), "огромная датаграмма должна давать ошибку");
        let next = read(&rx, 1000).await.expect("после ошибки следующая датаграмма должна читаться как обычно");
        assert_eq!(next, b"tail", "следующая датаграмма принесла не своё содержимое: остаток прошлой не должен лезть в новое чтение");
    }
}
