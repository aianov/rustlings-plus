// Свой удостоверяющий центр: сертификат CA, сертификат сервера и запись файлов с правильными правами.
// Здесь нет готовых проверок, только rcgen: допиши четыре функции.
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
use std::io::Write;
use std::path::Path;

pub struct Ca {
    cert: rcgen::Certificate,
    key: KeyPair,
}

pub struct Leaf {
    pub cert_pem: String,
    pub key_pem: String,
}

impl Ca {
    pub fn new(common_name: &str) -> Result<Ca, rcgen::Error> {
        let mut params = CertificateParams::new(Vec::new())?;
        params.distinguished_name.push(DnType::CommonName, common_name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key = KeyPair::generate()?;
        let cert = params.self_signed(&key)?;
        Ok(Ca { cert, key })
    }

    pub fn cert_pem(&self) -> String {
        self.cert.pem()
    }

    pub fn issue_server(&self, names: &[&str]) -> Result<Leaf, rcgen::Error> {
        let params = CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())?;
        let key = KeyPair::generate()?;
        let cert = params.signed_by(&key, &self.cert, &self.key)?;
        Ok(Leaf { cert_pem: cert.pem(), key_pem: key.serialize_pem() })
    }
}

pub fn write_pems(dir: &Path, ca: &Ca, leaf: &Leaf) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("ca.pem"), ca.cert_pem())?;
    std::fs::write(dir.join("server.pem"), &leaf.cert_pem)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(dir.join("server.key"))?.write_all(leaf.key_pem.as_bytes())
}
