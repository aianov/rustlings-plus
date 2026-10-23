// TLS-клиент: доверяет одному CA и проверяет имя сервера. Допиши request.
#![allow(unused_variables)]

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug)]
pub enum ClientError {
    InvalidName(String),
    BadCa(String),
    Io(String),
    Timeout,
}

pub async fn request(addr: SocketAddr, server_name: &str, ca_pem: &str, payload: &[u8], timeout: Duration) -> Result<Vec<u8>, ClientError> {
    todo!()
}
