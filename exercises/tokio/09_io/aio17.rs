// Клиент здоровается с сервером перед началом работы: отправляет строку `HELLO <имя>` и
// читает ответ `WELCOME <имя сервера>`. Сейчас функция умеет работать только с настоящим
// `TcpStream`, и протокол нельзя проверить в тесте без сети. Сделай её годной для любого
// потока, а когда тест на `duplex` заработает, проверь, всё ли в порядке с данными, которые
// сервер прислал сразу вслед за приветствием.
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;

async fn handshake(stream: &mut TcpStream, name: &str) -> io::Result<String> {
    stream.write_all(format!("HELLO {name}\n").as_bytes()).await?;
    let mut line = String::new();
    BufReader::new(&mut *stream).read_line(&mut line).await?;
    match line.trim_end().strip_prefix("WELCOME ") {
        Some(server) => Ok(server.to_string()),
        None => Err(io::Error::new(io::ErrorKind::InvalidData, "ожидалось WELCOME")),
    }
}

#[tokio::main]
async fn main() {
    let (mut client, mut server) = tokio::io::duplex(64);
    tokio::spawn(async move {
        server.write_all(b"WELCOME core\n").await.unwrap();
    });
    println!("{:?}", handshake(&mut client, "ann").await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn welcome_is_parsed() {
        let (mut client, mut server) = duplex(256);
        let peer = tokio::spawn(async move {
            let mut hello = [0u8; 10];
            server.read_exact(&mut hello).await.unwrap();
            assert_eq!(&hello, b"HELLO ann\n", "сервер должен получить приветствие клиента");
            server.write_all(b"WELCOME core\n").await.unwrap();
        });
        let name = handshake(&mut client, "ann").await.unwrap();
        assert_eq!(name, "core", "из WELCOME нужно вынуть имя сервера");
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn bytes_after_the_welcome_stay_in_the_stream() {
        let (mut client, mut server) = duplex(256);
        tokio::spawn(async move {
            let mut hello = [0u8; 10];
            server.read_exact(&mut hello).await.unwrap();
            server.write_all(b"WELCOME core\nSTATE 42\n").await.unwrap();
            server.shutdown().await.unwrap();
        });
        handshake(&mut client, "ann").await.unwrap();
        let mut rest = String::new();
        client.read_to_string(&mut rest).await.unwrap();
        assert_eq!(rest, "STATE 42\n", "всё, что сервер прислал после приветствия, должно остаться в потоке");
    }

    #[tokio::test]
    async fn wrong_greeting_is_invalid_data() {
        let (mut client, mut server) = duplex(256);
        tokio::spawn(async move {
            let mut hello = [0u8; 10];
            server.read_exact(&mut hello).await.unwrap();
            server.write_all(b"NOPE\n").await.unwrap();
        });
        let err = handshake(&mut client, "ann").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "ответ не WELCOME: ждём InvalidData");
    }
}
