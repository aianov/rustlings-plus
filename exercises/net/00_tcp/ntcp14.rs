// Эхо-сервер возвращает байты по мере чтения. Клиент должен отправить ему большой объём
// и собрать всё, что вернулось. Если делать это по очереди (сначала всё отправить, потом всё
// прочитать), на нескольких мегабайтах и клиент, и сервер зависают.
// TODO: Напиши upload_and_collect так, чтобы 8 МБ проходили без зависаний. Конец запроса
// обозначай закрытием записи. Верни всё, что вернул сервер.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn upload_and_collect(stream: TcpStream, data: Vec<u8>) -> io::Result<Vec<u8>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{} байт вернулось", upload_and_collect(stream, vec![1; 100]).await?.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    async fn echo_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    let _ = s.shutdown().await;
                    return;
                }
                if s.write_all(&buf[..n]).await.is_err() {
                    return;
                }
            }
        });
        addr
    }

    async fn roundtrip(len: usize) -> (Vec<u8>, Vec<u8>) {
        let addr = echo_server().await;
        let stream = TcpStream::connect(addr).await.unwrap();
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let echoed = timeout(Duration::from_secs(9), upload_and_collect(stream, data.clone()))
            .await
            .expect("не уложились в 9 секунд: похоже, клиент отправляет всё и только потом читает, и они с сервером ждут друг друга")
            .unwrap();
        (data, echoed)
    }

    #[tokio::test]
    async fn small_payload() {
        let (data, echoed) = roundtrip(1000).await;
        assert_eq!(echoed, data, "эхо вернулось не тем, что отправили");
    }

    #[tokio::test]
    async fn empty_payload() {
        let (_, echoed) = roundtrip(0).await;
        assert!(echoed.is_empty(), "отправили ничего, а вернулось {} байт", echoed.len());
    }

    #[tokio::test]
    async fn eight_megabytes() {
        let (data, echoed) = roundtrip(8 * 1024 * 1024).await;
        assert_eq!(echoed.len(), data.len(), "вернулось {} байт из {}", echoed.len(), data.len());
        assert!(echoed == data, "длина совпала, а содержимое нет");
    }
}
