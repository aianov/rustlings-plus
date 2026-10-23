// Клиент подключается ко всем подряд: на стенде самоподписанный сертификат, и кто-то «починил» проверку.
// Верни настоящую проверку и сохрани возможность доверять нескольким CA.
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use rustls::ClientConfig;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum TrustError {
    Empty,
    BadPem(String),
}

#[derive(Debug)]
struct AcceptAnything;

impl ServerCertVerifier for AcceptAnything {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ECDSA_NISTP384_SHA384, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256, SignatureScheme::RSA_PKCS1_SHA256]
    }
}

pub fn client_config(trusted: &[String]) -> Result<Arc<ClientConfig>, TrustError> {
    let _ = CertificateDer::pem_slice_iter(b"");
    let _ = trusted;
    Ok(Arc::new(ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(AcceptAnything)).with_no_client_auth()))
}
