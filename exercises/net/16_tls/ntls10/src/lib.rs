// Взаимный TLS: сервер требует сертификат клиента от своего CA, клиент его предъявляет.
// Допиши обе конфигурации.
#![allow(unused_variables)]

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
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
    todo!()
}

pub fn client_config(server_ca_pem: &str, identity: Option<(&str, &str)>) -> Result<Arc<ClientConfig>, String> {
    todo!()
}
