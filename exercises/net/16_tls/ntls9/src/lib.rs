// ALPN: сервер и клиент договариваются, на каком протоколе говорить, прямо в рукопожатии.
// Допиши конфигурации и функцию, которая узнаёт итог.
#![allow(unused_variables)]

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

pub fn server_config(cert_pem: &str, key_pem: &str) -> Arc<ServerConfig> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem.as_bytes()).collect::<Result<_, _>>().expect("сертификаты");
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).expect("ключ");
    let mut config = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).expect("пара ключ и сертификат");
    Arc::new(config)
}

pub fn client_config(ca_pem: &str, offer: &[&str]) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
        roots.add(cert.expect("CA")).expect("CA");
    }
    let mut config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    Arc::new(config)
}

pub async fn negotiate(addr: SocketAddr, name: &str, ca_pem: &str, offer: &[&str]) -> io::Result<Option<String>> {
    todo!()
}
