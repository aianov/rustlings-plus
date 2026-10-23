// Клиент хочет сказать «вот мой запрос, больше я ничего не добавлю» и дождаться ответа.
// Сервер читает запрос до конца потока (до EOF) и только потом отвечает. Если клиент не даст
// серверу EOF, они будут вечно ждать друг друга. Нужно закрыть только свою сторону записи,
// не трогая чтение.
// TODO: Допиши request_reply: отправь запрос, закрой запись и прочитай ответ до конца.

use std::io;
use tokio::net::TcpStream;

async fn request_reply(mut stream: TcpStream, request: &[u8]) -> io::Result<Vec<u8>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{:?}", request_reply(stream, b"hello").await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    async fn shouting_server() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            s.read_to_end(&mut request).await.unwrap();
            s.write_all(&request.to_ascii_uppercase()).await.unwrap();
        });
        addr
    }

    #[tokio::test]
    async fn gets_the_reply_after_half_close() {
        let addr = shouting_server().await;
        let stream = TcpStream::connect(addr).await.unwrap();
        let reply = timeout(Duration::from_secs(3), request_reply(stream, b"hello"))
            .await
            .expect("сервер ждёт EOF, а клиент ждёт ответ: после write_all нужен shutdown, иначе сервер не узнает, что запрос кончился")
            .unwrap();
        assert_eq!(reply, b"HELLO", "ответ не тот, что ждали");
    }

    #[tokio::test]
    async fn works_for_a_big_request_too() {
        let addr = shouting_server().await;
        let stream = TcpStream::connect(addr).await.unwrap();
        let request = vec![b'a'; 300_000];
        let reply = timeout(Duration::from_secs(5), request_reply(stream, &request))
            .await
            .expect("завис на большом запросе")
            .unwrap();
        assert_eq!(reply.len(), request.len(), "ответ обрезан: читать нужно до EOF, а не один раз");
        assert!(reply.iter().all(|b| *b == b'A'), "сервер должен был перевести всё в верхний регистр");
    }
}
