// Взаимный TLS: сервер требует сертификат клиента от своего CA, клиент его предъявляет.
// Допиши обе конфигурации.
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::sync::Arc;

fn roots(pem: &str) -> Result<RootCertStore, String> {
    let mut store = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
        store.add(cert.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    if store.is_empty() {
        return Err("в списке доверия нет ни одного сертификата".to_string());
    }
    Ok(store)
}

fn chain(pem: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    CertificateDer::pem_slice_iter(pem.as_bytes()).collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn server_config(cert_pem: &str, key_pem: &str, client_ca_pem: &str) -> Result<Arc<ServerConfig>, String> {
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots(client_ca_pem)?)).build().map_err(|e| e.to_string())?;
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| e.to_string())?;
    let config = ServerConfig::builder().with_client_cert_verifier(verifier).with_single_cert(chain(cert_pem)?, key).map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

pub fn client_config(server_ca_pem: &str, identity: Option<(&str, &str)>) -> Result<Arc<ClientConfig>, String> {
    let builder = ClientConfig::builder().with_root_certificates(roots(server_ca_pem)?);
    let config = match identity {
        Some((cert_pem, key_pem)) => {
            let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| e.to_string())?;
            builder.with_client_auth_cert(chain(cert_pem)?, key).map_err(|e| e.to_string())?
        }
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
}
