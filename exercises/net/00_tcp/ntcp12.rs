// Сетевые ошибки приходят как io::Error, и смысл у них в kind(): сервер не слушает (Refused),
// собеседник оборвал соединение (Reset), закрылся раньше времени (UnexpectedEof), запись в
// закрытый сокет (BrokenPipe), вышло время. Выше по стеку нужен свой enum с понятными словами.
// TODO: Допиши Display для NetError и From<io::Error>, который раскладывает kind по вариантам.
// Остальное (connect и read_n) уже написано, обрати внимание, как там работает `?`.

use std::fmt;
use std::io;
use std::net::SocketAddr;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

#[derive(Debug)]
enum NetError {
    Refused,
    Reset,
    Closed,
    BrokenPipe,
    TimedOut,
    Other(io::Error),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        todo!()
    }
}

async fn connect(addr: SocketAddr) -> Result<TcpStream, NetError> {
    Ok(TcpStream::connect(addr).await?)
}

async fn read_n(stream: &mut TcpStream, n: usize) -> Result<Vec<u8>, NetError> {
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

#[tokio::main]
async fn main() {
    match connect("127.0.0.1:1".parse().unwrap()).await {
        Ok(mut stream) => println!("{:?}", read_n(&mut stream, 4).await),
        Err(e) => println!("не вышло: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    #[test]
    fn kinds_map_to_variants() {
        let cases = [
            (ErrorKind::ConnectionRefused, "Refused"),
            (ErrorKind::ConnectionReset, "Reset"),
            (ErrorKind::UnexpectedEof, "Closed"),
            (ErrorKind::BrokenPipe, "BrokenPipe"),
            (ErrorKind::TimedOut, "TimedOut"),
        ];
        for (kind, want) in cases {
            let got = format!("{:?}", NetError::from(io::Error::from(kind)));
            assert_eq!(got, want, "{kind:?} должен превращаться в NetError::{want}, а получилось {got}");
        }
    }

    #[test]
    fn unknown_kinds_keep_the_original_error() {
        match NetError::from(io::Error::new(ErrorKind::PermissionDenied, "нельзя")) {
            NetError::Other(e) => assert_eq!(e.kind(), ErrorKind::PermissionDenied, "Other должен хранить исходную ошибку"),
            other => panic!("PermissionDenied не из нашего списка, ждали Other, получили {other:?}"),
        }
    }

    #[test]
    fn messages_are_distinct_and_useful() {
        let all = [
            NetError::Refused,
            NetError::Reset,
            NetError::Closed,
            NetError::BrokenPipe,
            NetError::TimedOut,
            NetError::Other(io::Error::other("диск сгорел")),
        ];
        let texts: Vec<String> = all.iter().map(|e| e.to_string()).collect();
        for (i, a) in texts.iter().enumerate() {
            assert!(!a.trim().is_empty(), "у варианта {i} пустое сообщение");
            for b in &texts[i + 1..] {
                assert_ne!(a, b, "два разных варианта показывают одно и то же сообщение: «{a}»");
            }
        }
        assert!(texts[5].contains("диск сгорел"), "Other должен показывать текст исходной ошибки, а не терять его");
    }

    #[tokio::test]
    async fn connect_to_a_dead_port_is_refused() {
        let dead = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let res = timeout(Duration::from_secs(5), connect(dead)).await.expect("завис");
        assert!(matches!(res, Err(NetError::Refused)), "порт никто не слушает, ждали Refused, а вышло {res:?}");
    }

    #[tokio::test]
    async fn early_close_is_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(b"ab").await.unwrap();
        });
        let mut client = connect(addr).await.unwrap();
        let res = timeout(Duration::from_secs(5), read_n(&mut client, 4)).await.expect("завис");
        assert!(matches!(res, Err(NetError::Closed)), "сервер прислал 2 байта из 4 и закрылся, ждали Closed, а вышло {res:?}");
    }

    #[tokio::test]
    async fn dropping_unread_data_resets_the_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            s.readable().await.unwrap();
            drop(s);
        });
        let mut client = connect(addr).await.unwrap();
        client.write_all(b"unread").await.unwrap();
        server.await.unwrap();
        let res = timeout(Duration::from_secs(5), read_n(&mut client, 1)).await.expect("завис");
        assert!(matches!(res, Err(NetError::Reset)), "сервер закрыл сокет с непрочитанными данными, ждали Reset, а вышло {res:?}");
    }
}
