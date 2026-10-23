use std::io;
use std::sync::Arc;

use rustls::crypto::ring::default_provider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

fn invalid(what: &str, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{what}: {error}"))
}

pub fn server_config(cert_pem: &[u8], key_pem: &[u8]) -> io::Result<Arc<ServerConfig>> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<_, _>>()
        .map_err(|e| invalid("сертификат", e))?;
    let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(|e| invalid("ключ", e))?;
    let mut config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| invalid("пара сертификат и ключ", e))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

pub fn client_config(ca_pem: &[u8]) -> io::Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem) {
        let cert = cert.map_err(|e| invalid("CA", e))?;
        roots.add(cert).map_err(|e| invalid("CA", e))?;
    }
    let mut config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
