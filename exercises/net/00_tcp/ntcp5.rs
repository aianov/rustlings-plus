// Самый простой обмен: записать запрос и прочитать ответ. Для записи берём write_all,
// для чтения read в буфер: он возвращает, сколько байтов реально легло в начало буфера.
// TODO: Допиши ask: отправь question целиком, прочитай ответ (один read в буфер на 64 байта)
// и верни ровно те байты, что пришли. Остальной хвост буфера это мусор.

use std::io;
use tokio::net::TcpStream;

async fn ask(stream: &mut TcpStream, question: &[u8]) -> io::Result<Vec<u8>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{:?}", ask(&mut stream, b"ping").await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    async fn echo_prefix_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            loop {
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                let mut reply = b"echo:".to_vec();
                reply.extend_from_slice(&buf[..n]);
                s.write_all(&reply).await.unwrap();
            }
        });
        addr
    }

    #[tokio::test]
    async fn one_question_one_answer() {
        let addr = echo_prefix_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let answer = timeout(Duration::from_secs(5), ask(&mut stream, b"ping"))
            .await
            .expect("ответа не дождались: ask должна сначала отправить вопрос (write_all), потом читать")
            .unwrap();
        assert_eq!(answer, b"echo:ping", "ask вернула не те байты (хвост буфера обрезан по n?)");
    }

    #[tokio::test]
    async fn stream_is_reusable() {
        let addr = echo_prefix_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        for word in ["a", "bcd"] {
            let answer = timeout(Duration::from_secs(5), ask(&mut stream, word.as_bytes()))
                .await
                .expect("второй вопрос по тому же соединению завис")
                .unwrap();
            assert_eq!(answer, format!("echo:{word}").into_bytes(), "ответ на «{word}» не тот");
        }
    }
}
