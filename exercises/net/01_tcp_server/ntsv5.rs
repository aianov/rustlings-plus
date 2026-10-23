// Сервер-сессия делает два дела сразу: каждые every секунд присылает клиенту строку «tick N»
// (всего ticks штук) и параллельно читает всё, что клиент пишет, пока тот не закроет запись.
// Если сначала читать, а потом писать, клиент, который ждёт первый tick, чтобы закрыть запись,
// повиснет вместе с сервером.
// TODO: Допиши session так, чтобы чтение и запись шли независимо друг от друга. Верни всё, что
// клиент прислал. После последнего tick клиент должен узнать, что строк больше не будет.

use std::io;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

async fn session(stream: TcpStream, ticks: u32, every: Duration) -> io::Result<Vec<u8>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:7").await?;
    let got = session(stream, 3, Duration::from_secs(1)).await?;
    println!("клиент прислал {} байт", got.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    #[tokio::test]
    async fn reads_and_writes_at_the_same_time() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            session(stream, 3, Duration::from_millis(30)).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut first = [0u8; 7];
        timeout(Duration::from_secs(3), client.read_exact(&mut first))
            .await
            .expect("клиент так и не получил первый tick: сервер читает и ждёт EOF, а писать начнёт потом. Читай и пиши одновременно")
            .unwrap();
        assert_eq!(&first, b"tick 1\n", "первая строка от сервера");
        client.write_all(b"hello").await.unwrap();
        client.shutdown().await.unwrap();
        let mut rest = Vec::new();
        timeout(Duration::from_secs(3), client.read_to_end(&mut rest)).await.expect("сервер не закрыл запись после последнего tick").unwrap();
        assert_eq!(rest, b"tick 2\ntick 3\n", "оставшиеся tick должны дойти по порядку");
        let received = timeout(Duration::from_secs(3), server).await.expect("session не вернулась").unwrap().unwrap();
        assert_eq!(received, b"hello", "session должна вернуть всё, что прислал клиент");
    }

    #[tokio::test]
    async fn zero_ticks_is_just_a_reader() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            session(stream, 0, Duration::from_millis(10)).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"abc").await.unwrap();
        client.shutdown().await.unwrap();
        let mut rest = Vec::new();
        timeout(Duration::from_secs(3), client.read_to_end(&mut rest)).await.expect("завис").unwrap();
        assert!(rest.is_empty(), "tick не должно быть, а пришло {rest:?}");
        assert_eq!(timeout(Duration::from_secs(3), server).await.expect("завис").unwrap().unwrap(), b"abc");
    }
}
