// На старте сервис читает цепочку сертификатов и закрытый ключ. Когда файлы не те, ошибка должна
// говорить, что именно не так. Допиши загрузку и сборку конфигурации.
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum TlsError {
    NoCertificates,
    NoPrivateKey,
    BadPem(String),
    KeyMismatch(String),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsError::NoCertificates => write!(f, "в файле нет ни одного сертификата"),
            TlsError::NoPrivateKey => write!(f, "в файле нет закрытого ключа"),
            TlsError::BadPem(why) => write!(f, "файл повреждён: {why}"),
            TlsError::KeyMismatch(why) => write!(f, "закрытый ключ не подходит к сертификату: {why}"),
        }
    }
}

impl std::error::Error for TlsError {}

pub fn load_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let certs = CertificateDer::pem_slice_iter(pem).collect::<Result<Vec<_>, _>>().map_err(|e| TlsError::BadPem(e.to_string()))?;
    if certs.is_empty() {
        return Err(TlsError::NoCertificates);
    }
    Ok(certs)
}

pub fn load_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, TlsError> {
    PrivateKeyDer::from_pem_slice(pem).map_err(|e| match e {
        rustls::pki_types::pem::Error::NoItemsFound => TlsError::NoPrivateKey,
        other => TlsError::BadPem(other.to_string()),
    })
}

pub fn server_config(cert_pem: &[u8], key_pem: &[u8]) -> Result<Arc<ServerConfig>, TlsError> {
    let certs = load_certs(cert_pem)?;
    let key = load_key(key_pem)?;
    let config = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).map_err(|e| TlsError::KeyMismatch(e.to_string()))?;
    Ok(Arc::new(config))
}
