// Самый короткий сервер на свете: любому запросу отвечает "hello". Сейчас serve_one не делает ничего,
// и браузер крутит колёсико. Но отвечать сразу после первого read нельзя: голова запроса приходит
// кусками, а ответ, отправленный раньше времени, клиент может и не дождаться.
// TODO: Допиши serve_one: дождись конца головы запроса, ответь 200 с телом "hello" и закрой соединение.
// Оборванного на полуслове клиента надо отпустить без ошибки и без зависания.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn serve_one(mut stream: TcpStream) -> io::Result<()> {
    todo!()
}

async fn serve(listener: TcpListener) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let _ = serve_one(stream).await;
        });
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("слушаю на {}", listener.local_addr()?);
    serve(listener).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn start() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener));
        addr
    }

    async fn finish(mut client: TcpStream) -> String {
        let mut all = Vec::new();
        timeout(Duration::from_secs(2), client.read_to_end(&mut all))
            .await
            .expect("сервер не закрыл соединение после ответа")
            .unwrap();
        String::from_utf8(all).unwrap()
    }

    #[tokio::test]
    async fn waits_for_the_end_of_the_head() {
        let mut client = TcpStream::connect(start().await).await.unwrap();
        client.write_all(b"GET / HTTP/1.1\r\nHost: a\r\n").await.unwrap();
        let mut probe = [0u8; 64];
        assert!(
            timeout(Duration::from_millis(150), client.read(&mut probe)).await.is_err(),
            "сервер ответил, не дождавшись пустой строки после заголовков"
        );
        client.write_all(b"\r\n").await.unwrap();
        let text = finish(client).await;
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "первая строка ответа: {text:?}");
        assert!(text.contains("Content-Length: 5\r\n"), "без длины клиент не поймёт, где конец тела: {text:?}");
        assert!(text.ends_with("\r\n\r\nhello"), "тело должно быть hello: {text:?}");
    }

    #[tokio::test]
    async fn terminator_may_arrive_in_pieces() {
        let mut client = TcpStream::connect(start().await).await.unwrap();
        client.set_nodelay(true).unwrap();
        for piece in ["GET / HTTP/1.1\r\nHost: a\r\n\r", "\n"] {
            client.write_all(piece.as_bytes()).await.unwrap();
            client.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        let text = finish(client).await;
        assert!(text.ends_with("hello"), "\\r\\n\\r\\n разрезан между двумя read, искать его надо во всём накопленном: {text:?}");
    }

    #[tokio::test]
    async fn abandoned_client_is_released() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let handle = tokio::spawn(serve_one(stream));
        let mut client = client;
        client.write_all(b"GET / HT").await.unwrap();
        drop(client);
        let result = timeout(Duration::from_secs(1), handle).await.expect("serve_one завис на оборванном клиенте").unwrap();
        assert!(result.is_ok(), "клиент ушёл на полуслове: это не ошибка сервера, но {result:?}");
    }
}
