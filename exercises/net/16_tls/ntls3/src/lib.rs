// TLS-эхо-сервер. Рукопожатие занимает время и может зависнуть, поэтому оно не должно блокировать приём
// других соединений. Допиши serve.
#![allow(unused_variables)]

use rustls::ServerConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub async fn serve(listener: TcpListener, config: Arc<ServerConfig>, handshake_timeout: Duration) {
    todo!()
}
