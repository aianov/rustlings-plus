// Обработчик не знает, что под ним TLS. Подключи шифрование, не меняя обработчик.
use rustls::ServerConfig;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub async fn handle<S: AsyncRead + AsyncWrite + Unpin>(stream: S) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await? == 0 || header == "\r\n" {
            break;
        }
    }
    let body = if request_line.starts_with("GET /hello") { "привет из обработчика" } else { "не найдено" };
    let status = if body == "не найдено" { "404 Not Found" } else { "200 OK" };
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    reader.get_mut().write_all(response.as_bytes()).await?;
    reader.get_mut().flush().await
}

pub async fn serve_plain(listener: TcpListener) {
    loop {
        let Ok((tcp, _)) = listener.accept().await else { continue };
        tokio::spawn(async move {
            let _ = handle(tcp).await;
        });
    }
}

pub async fn serve_tls(listener: TcpListener, config: Arc<ServerConfig>) {
    let acceptor = TlsAcceptor::from(config);
    loop {
        let Ok((tcp, _)) = listener.accept().await else { continue };
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let Ok(mut tls) = acceptor.accept(tcp).await else { return };
            let _ = handle(&mut tls).await;
            let _ = tls.shutdown().await;
        });
    }
}
