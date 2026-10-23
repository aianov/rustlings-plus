use std::io;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

async fn read_message(stream: &mut TcpStream, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{} байт", read_message(&mut stream, 1000).await?.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    async fn trickling_server(chunks: usize, chunk_len: usize) -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            for i in 0..chunks {
                s.write_all(&vec![i as u8; chunk_len]).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        TcpStream::connect(addr).await.unwrap()
    }

    #[tokio::test]
    async fn waits_for_the_whole_message() {
        let mut stream = trickling_server(5, 200).await;
        let msg = timeout(Duration::from_secs(5), read_message(&mut stream, 1000))
            .await
            .expect("завис")
            .unwrap();
        assert_eq!(msg.len(), 1000, "пришло {} байт из 1000: read отдаёт то, что есть, а не то, что просили", msg.len());
        assert!(msg[..200].iter().all(|b| *b == 0) && msg[800..].iter().all(|b| *b == 4), "содержимое перепутано");
    }

    #[tokio::test]
    async fn early_close_is_an_error_not_a_short_message() {
        let mut stream = trickling_server(1, 10).await;
        let res = timeout(Duration::from_secs(5), read_message(&mut stream, 100)).await.expect("завис");
        let err = res.expect_err("сервер закрылся, не дослав сообщение: обрезанное сообщение нельзя выдавать за целое");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ожидали UnexpectedEof, получили {err}");
    }
}
