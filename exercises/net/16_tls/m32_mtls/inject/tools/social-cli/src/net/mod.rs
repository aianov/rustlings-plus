use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName};
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

pub enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(tcp) => Pin::new(tcp).poll_read(cx, buf),
            Stream::Tls(tls) => Pin::new(tls.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(tcp) => Pin::new(tcp).poll_write(cx, buf),
            Stream::Tls(tls) => Pin::new(tls.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(tcp) => Pin::new(tcp).poll_flush(cx),
            Stream::Tls(tls) => Pin::new(tls.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(tcp) => Pin::new(tcp).poll_shutdown(cx),
            Stream::Tls(tls) => Pin::new(tls.as_mut()).poll_shutdown(cx),
        }
    }
}

#[derive(Clone, Default)]
pub struct Dialer {
    tls: Option<TlsConnector>,
}

fn invalid(what: &str, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{what}: {error}"))
}

impl Dialer {
    pub fn plain() -> Dialer {
        Dialer::default()
    }

    pub fn trusting(ca_pem: &[u8]) -> io::Result<Dialer> {
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(ca_pem) {
            let cert = cert.map_err(|e| invalid("CA", e))?;
            roots.add(cert).map_err(|e| invalid("CA", e))?;
        }
        if roots.is_empty() {
            return Err(invalid("CA", "в файле нет ни одного сертификата"));
        }
        let mut config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| invalid("TLS", e))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Dialer {
            tls: Some(TlsConnector::from(Arc::new(config))),
        })
    }

    pub fn is_tls(&self) -> bool {
        self.tls.is_some()
    }

    pub fn http_scheme(&self) -> &'static str {
        if self.is_tls() { "https" } else { "http" }
    }

    pub async fn connect(&self, addr: &str) -> io::Result<Stream> {
        let tcp = TcpStream::connect(addr).await?;
        tcp.set_nodelay(true)?;
        let Some(tls) = &self.tls else {
            return Ok(Stream::Plain(tcp));
        };
        let host = addr.rsplit_once(':').map_or(addr, |(host, _)| host);
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let name = ServerName::try_from(host.to_string()).map_err(|e| invalid("имя сервера", e))?;
        let stream = tls.connect(name, tcp).await?;
        Ok(Stream::Tls(Box::new(stream)))
    }
}
