// Клиент подключается ко всем подряд: на стенде самоподписанный сертификат, и кто-то «починил» проверку.
// Верни настоящую проверку и сохрани возможность доверять нескольким CA.
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use rustls::RootCertStore;
use rustls::ClientConfig;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum TrustError {
    Empty,
    BadPem(String),
}

pub fn client_config(trusted: &[String]) -> Result<Arc<ClientConfig>, TrustError> {
    let mut roots = RootCertStore::empty();
    for pem in trusted {
        for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
            let cert = cert.map_err(|e| TrustError::BadPem(e.to_string()))?;
            roots.add(cert).map_err(|e| TrustError::BadPem(e.to_string()))?;
        }
    }
    if roots.is_empty() {
        return Err(TrustError::Empty);
    }
    Ok(Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()))
}
