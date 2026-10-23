use std::io;
use std::sync::Arc;

use rustls::crypto::ring::default_provider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, RootCertStore, ServerConfig};

fn invalid(what: &str, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{what}: {error}"))
}

fn chain(cert_pem: &[u8]) -> io::Result<Vec<CertificateDer<'static>>> {
    CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<_, _>>()
        .map_err(|e| invalid("сертификат", e))
}

fn private_key(key_pem: &[u8]) -> io::Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_slice(key_pem).map_err(|e| invalid("ключ", e))
}

fn roots(ca_pem: &[u8]) -> io::Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem) {
        let cert = cert.map_err(|e| invalid("CA", e))?;
        roots.add(cert).map_err(|e| invalid("CA", e))?;
    }
    Ok(roots)
}

fn finish_server(mut config: ServerConfig) -> Arc<ServerConfig> {
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

fn finish_client(mut config: ClientConfig) -> Arc<ClientConfig> {
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

pub fn server_config(cert_pem: &[u8], key_pem: &[u8]) -> io::Result<Arc<ServerConfig>> {
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_no_client_auth()
        .with_single_cert(chain(cert_pem)?, private_key(key_pem)?)
        .map_err(|e| invalid("пара сертификат и ключ", e))?;
    Ok(finish_server(config))
}

pub fn mutual_server_config(
    cert_pem: &[u8],
    key_pem: &[u8],
    ca_pem: &[u8],
) -> io::Result<Arc<ServerConfig>> {
    let provider = Arc::new(default_provider());
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(roots(ca_pem)?), provider.clone())
            .build()
            .map_err(|e| invalid("проверка клиентов", e))?;
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(chain(cert_pem)?, private_key(key_pem)?)
        .map_err(|e| invalid("пара сертификат и ключ", e))?;
    Ok(finish_server(config))
}

pub fn client_config(ca_pem: &[u8]) -> io::Result<Arc<ClientConfig>> {
    let config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_root_certificates(roots(ca_pem)?)
        .with_no_client_auth();
    Ok(finish_client(config))
}

pub fn mutual_client_config(
    ca_pem: &[u8],
    cert_pem: &[u8],
    key_pem: &[u8],
) -> io::Result<Arc<ClientConfig>> {
    let config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| invalid("версии TLS", e))?
        .with_root_certificates(roots(ca_pem)?)
        .with_client_auth_cert(chain(cert_pem)?, private_key(key_pem)?)
        .map_err(|e| invalid("пара сертификат и ключ", e))?;
    Ok(finish_client(config))
}
