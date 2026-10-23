// Рукопожатие не удалось, а причин пять. Научись различать их по ошибке и писать в журнал так,
// чтобы дежурный сразу знал, что чинить. Допиши classify и describe.
#![allow(unused_variables)]

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{CertificateError, ClientConfig, RootCertStore};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyFailure {
    UnknownIssuer,
    NameMismatch,
    Expired,
    NotYetValid,
    Other(String),
}

pub async fn try_connect(addr: SocketAddr, name: &str, ca_pem: &str) -> io::Result<()> {
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
        roots.add(cert.map_err(io::Error::other)?).map_err(io::Error::other)?;
    }
    let config = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
    let name = ServerName::try_from(name.to_string()).map_err(io::Error::other)?;
    let tcp = TcpStream::connect(addr).await?;
    TlsConnector::from(config).connect(name, tcp).await.map(|_| ())
}

pub fn classify(err: &io::Error) -> VerifyFailure {
    todo!()
}

pub fn describe(failure: &VerifyFailure) -> String {
    todo!()
}
