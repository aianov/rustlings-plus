// UDP-эхо: сервер получает датаграмму и отправляет её обратно тому, кто прислал. Звучит как эхо из
// главы про TCP, но здесь нет потока, нет соединения и нет «конца данных».
// TODO: Напиши с нуля echo_n: обслужи ровно count датаграмм от любых клиентов и верни, сколько
// всего байтов отправлено обратно. Каждая датаграмма возвращается отдельной датаграммой.

use std::io;
use tokio::net::UdpSocket;

async fn echo_n(socket: UdpSocket, count: usize) -> io::Result<usize> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    println!("слушаем {}", socket.local_addr()?);
    println!("эхо отправило {} байт", echo_n(socket, 1).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn server(count: usize) -> (SocketAddr, tokio::task::JoinHandle<io::Result<usize>>) {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        (addr, tokio::spawn(echo_n(socket, count)))
    }

    async fn recv(client: &UdpSocket) -> Vec<u8> {
        let mut buf = [0u8; 256];
        let (n, _) = timeout(Duration::from_secs(3), client.recv_from(&mut buf))
            .await
            .expect("ответ не пришёл за 3 секунды: сервер потерял датаграмму или остановился раньше времени")
            .unwrap();
        buf[..n].to_vec()
    }

    #[tokio::test]
    async fn every_datagram_comes_back_separately() {
        let (addr, task) = server(3).await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for part in [&b"one"[..], b"", b"three!!"] {
            client.send_to(part, addr).await.unwrap();
        }
        assert_eq!(recv(&client).await, b"one", "первая датаграмма вернулась не такой, какой ушла");
        assert_eq!(recv(&client).await, b"", "пустая датаграмма это такое же сообщение, как остальные, её надо вернуть, а не считать концом");
        assert_eq!(recv(&client).await, b"three!!", "границы датаграмм должны сохраняться: три отправки, три ответа");
        let total = timeout(Duration::from_secs(3), task).await.expect("echo_n не завершился после count датаграмм").unwrap().unwrap();
        assert_eq!(total, 10, "echo_n должна вернуть число отправленных обратно байтов (3 + 0 + 7)");
    }

    #[tokio::test]
    async fn replies_go_to_the_right_client() {
        let (addr, task) = server(4).await;
        let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        a.send_to(b"a1", addr).await.unwrap();
        b.send_to(b"b1", addr).await.unwrap();
        a.send_to(b"a2", addr).await.unwrap();
        b.send_to(b"b2", addr).await.unwrap();
        assert_eq!(recv(&a).await, b"a1");
        assert_eq!(recv(&a).await, b"a2", "клиент a получил чужое или не то: ответ уходит тому адресу, откуда пришёл запрос");
        assert_eq!(recv(&b).await, b"b1");
        assert_eq!(recv(&b).await, b"b2");
        timeout(Duration::from_secs(3), task).await.unwrap().unwrap().unwrap();
    }
}
