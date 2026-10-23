use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

#[derive(Debug)]
pub enum EchoError {
    Refused,
    Reset,
    Closed,
    TimedOut,
    Io(io::Error),
}

impl fmt::Display for EchoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EchoError::Refused => f.write_str("сервер не принимает соединения: там никто не слушает порт"),
            EchoError::Reset => f.write_str("соединение оборвал собеседник (reset)"),
            EchoError::Closed => f.write_str("собеседник закрыл соединение раньше, чем договорились"),
            EchoError::TimedOut => f.write_str("собеседник молчит дольше допустимого"),
            EchoError::Io(e) => write!(f, "ошибка ввода-вывода: {e}"),
        }
    }
}

impl std::error::Error for EchoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EchoError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for EchoError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::ConnectionRefused => EchoError::Refused,
            io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => EchoError::Reset,
            io::ErrorKind::UnexpectedEof => EchoError::Closed,
            io::ErrorKind::TimedOut => EchoError::TimedOut,
            _ => EchoError::Io(e),
        }
    }
}

pub async fn serve_one(listener: &TcpListener, idle: Duration) -> Result<u64, EchoError> {
    let (mut stream, _) = listener.accept().await?;
    let mut buf = vec![0u8; 16 * 1024];
    let mut total = 0u64;
    loop {
        let n = match timeout(idle, stream.read(&mut buf)).await {
            Ok(read) => read?,
            Err(_) => return Err(EchoError::TimedOut),
        };
        if n == 0 {
            stream.shutdown().await?;
            return Ok(total);
        }
        stream.write_all(&buf[..n]).await?;
        total += n as u64;
    }
}

pub async fn echo_roundtrip(addr: SocketAddr, data: Vec<u8>) -> Result<Vec<u8>, EchoError> {
    let stream = TcpStream::connect(addr).await?;
    let (mut reader, mut writer) = stream.into_split();
    let expected = data.len();
    let sender = tokio::spawn(async move {
        writer.write_all(&data).await?;
        writer.shutdown().await
    });
    let mut echoed = Vec::with_capacity(expected);
    reader.read_to_end(&mut echoed).await?;
    sender.await.map_err(io::Error::other)??;
    if echoed.len() != expected {
        return Err(EchoError::Closed);
    }
    Ok(echoed)
}
