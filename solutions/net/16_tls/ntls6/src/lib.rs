// Рукопожатие не удалось, а причин пять. Научись различать их по ошибке и писать в журнал так,
// чтобы дежурный сразу знал, что чинить. Допиши classify и describe.
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
    let inner = err.get_ref().and_then(|e| e.downcast_ref::<rustls::Error>());
    match inner {
        Some(rustls::Error::InvalidCertificate(why)) => match why {
            CertificateError::UnknownIssuer => VerifyFailure::UnknownIssuer,
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => VerifyFailure::NameMismatch,
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => VerifyFailure::Expired,
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => VerifyFailure::NotYetValid,
            other => VerifyFailure::Other(format!("{other:?}")),
        },
        _ => VerifyFailure::Other(err.to_string()),
    }
}

pub fn describe(failure: &VerifyFailure) -> String {
    match failure {
        VerifyFailure::UnknownIssuer => "сертификат выдан неизвестным центром: проверь, что в списке доверия лежит нужный CA".to_string(),
        VerifyFailure::NameMismatch => "сертификат выдан на другое имя: сравни SAN сертификата с именем, к которому подключаешься".to_string(),
        VerifyFailure::Expired => "срок действия сертификата истёк: перевыпусти сертификат".to_string(),
        VerifyFailure::NotYetValid => "срок действия сертификата ещё не начался: проверь часы на сервере или клиенте".to_string(),
        VerifyFailure::Other(why) => format!("рукопожатие не удалось не из-за сертификата: {why}"),
    }
}
