// send_all должна отправить в сокет все байты. Маленькие сообщения она отправляет отлично.
// А вот на большом объёме часть данных до собеседника не доходит, и никакой ошибки нет.
// Найди причину и исправь.

use std::io;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

async fn send_all(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    stream.write(data).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    send_all(&mut stream, b"hello").await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    async fn receive_everything(listener: TcpListener) -> (usize, u64) {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let (mut count, mut sum) = (0usize, 0u64);
        loop {
            let n = s.read(&mut buf).await.unwrap();
            if n == 0 {
                return (count, sum);
            }
            count += n;
            sum += buf[..n].iter().map(|b| u64::from(*b)).sum::<u64>();
        }
    }

    async fn transfer(data: Vec<u8>) -> (usize, u64) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let receiver = tokio::spawn(receive_everything(listener));
        let mut client = TcpStream::connect(addr).await.unwrap();
        send_all(&mut client, &data).await.unwrap();
        client.shutdown().await.unwrap();
        receiver.await.unwrap()
    }

    #[tokio::test]
    async fn small_message_arrives() {
        let got = timeout(Duration::from_secs(5), transfer(b"hello".to_vec())).await.expect("завис");
        assert_eq!(got.0, 5, "даже маленькое сообщение дошло не целиком");
    }

    #[tokio::test]
    async fn sixteen_megabytes_arrive_whole() {
        let data: Vec<u8> = (0..16 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let expected_sum: u64 = data.iter().map(|b| u64::from(*b)).sum();
        let (count, sum) = timeout(Duration::from_secs(9), transfer(data.clone())).await.expect("передача 16 МБ не уложилась в 9 секунд");
        assert_eq!(count, data.len(), "дошло {count} байт из {}: write принял только часть буфера, а хвост потерян", data.len());
        assert_eq!(sum, expected_sum, "размер совпал, но содержимое отличается");
    }
}
