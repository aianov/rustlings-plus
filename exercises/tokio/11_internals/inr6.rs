// Два обработчика ждут данные на одном сокете (у них общий `Arc<TcpStream>`). Клиент присылает
// два сообщения с паузой. Иногда один из обработчиков падает с ошибкой «Resource temporarily
// unavailable», хотя сокет цел, никто его не закрывал, а `readable()` честно вернул `Ok`.
// Объясни это себе и сделай `read_some` устойчивым: она должна вернуть данные, когда они придут.
use std::io;
use tokio::net::TcpStream;

async fn read_some(sock: &TcpStream, buf: &mut [u8]) -> io::Result<usize> {
    sock.readable().await?;
    sock.try_read(buf)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _client = TcpStream::connect(addr).await.unwrap();
    println!("{addr}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    async fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (client, server) = tokio::join!(TcpStream::connect(addr), listener.accept());
        (client.unwrap(), server.unwrap().0)
    }

    fn reader(sock: &Arc<TcpStream>) -> tokio::task::JoinHandle<io::Result<Vec<u8>>> {
        let sock = sock.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 16];
            let n = read_some(&sock, &mut buf).await?;
            Ok(buf[..n].to_vec())
        })
    }

    #[tokio::test]
    async fn two_readers_share_two_messages() {
        let (mut client, server) = pair().await;
        let server = Arc::new(server);
        let a = reader(&server);
        let b = reader(&server);
        sleep(Duration::from_millis(50)).await;
        client.write_all(b"ab").await.unwrap();
        sleep(Duration::from_millis(50)).await;
        client.write_all(b"cd").await.unwrap();
        let (ra, rb) = timeout(Duration::from_secs(5), async { tokio::join!(a, b) })
            .await
            .expect("один из обработчиков так и не получил своё сообщение");
        let first = ra.unwrap().expect("обработчик получил ошибку вместо ожидания: готовность была ложной, а ты это не учла");
        let second = rb.unwrap().expect("обработчик получил ошибку вместо ожидания: готовность была ложной, а ты это не учла");
        let mut got = vec![first, second];
        got.sort();
        assert_eq!(got, vec![b"ab".to_vec(), b"cd".to_vec()], "каждое сообщение достаётся ровно одному обработчику");
    }

    #[tokio::test]
    async fn end_of_stream_is_zero_bytes_not_a_hang() {
        let (client, server) = pair().await;
        drop(client);
        let mut buf = [0u8; 8];
        let n = timeout(Duration::from_secs(5), read_some(&server, &mut buf))
            .await
            .expect("закрытый сокет должен давать конец потока, а не вечное ожидание")
            .unwrap();
        assert_eq!(n, 0, "конец потока это Ok(0)");
    }
}
