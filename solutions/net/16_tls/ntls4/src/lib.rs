// TLS-клиент: доверяет одному CA и проверяет имя сервера. Допиши request.
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
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
        let cert = cert.map_err(|e| ClientError::BadCa(e.to_string()))?;
        roots.add(cert).map_err(|e| ClientError::BadCa(e.to_string()))?;
    }
    if roots.is_empty() {
        return Err(ClientError::BadCa("в списке доверия нет ни одного сертификата".to_string()));
    }
    let name = ServerName::try_from(server_name.to_string()).map_err(|e| ClientError::InvalidName(e.to_string()))?;
    let connector = TlsConnector::from(Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()));
    let exchange = async {
        let tcp = TcpStream::connect(addr).await?;
        let mut tls = connector.connect(name, tcp).await?;
        tls.write_all(payload).await?;
        tls.shutdown().await?;
        let mut reply = Vec::new();
        tls.read_to_end(&mut reply).await?;
        Ok::<_, std::io::Error>(reply)
    };
    match tokio::time::timeout(timeout, exchange).await {
        Err(_) => Err(ClientError::Timeout),
        Ok(Err(e)) => Err(ClientError::Io(e.to_string())),
        Ok(Ok(reply)) => Ok(reply),
    }
}
