// TLS-эхо-сервер. Рукопожатие занимает время и может зависнуть, поэтому оно не должно блокировать приём
// других соединений. Допиши serve.
use rustls::ServerConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub async fn serve(listener: TcpListener, config: Arc<ServerConfig>, handshake_timeout: Duration) {
    let acceptor = TlsAcceptor::from(config);
    loop {
        let Ok((tcp, _)) = listener.accept().await else { continue };
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let Ok(Ok(mut tls)) = tokio::time::timeout(handshake_timeout, acceptor.accept(tcp)).await else { return };
            let mut buf = [0u8; 4096];
            loop {
                match tls.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tls.write_all(&buf[..n]).await.is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = tls.shutdown().await;
        });
    }
}
