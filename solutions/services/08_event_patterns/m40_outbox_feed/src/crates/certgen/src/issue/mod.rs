use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};

pub const CA: &str = "ca";

pub const SERVICES: [&str; 7] = [
    "api-gateway",
    "auth-service",
    "user-service",
    "post-service",
    "chat-service",
    "notify-service",
    "feed-service",
];

fn other(error: rcgen::Error) -> io::Error {
    io::Error::other(error)
}

fn write(path: &Path, text: &str, private: bool) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    options.open(path)?.write_all(text.as_bytes())
}

pub fn issue_all(dir: &Path, force: bool) -> io::Result<Vec<PathBuf>> {
    let ca_pem = dir.join(format!("{CA}.pem"));
    if ca_pem.exists() && !force {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} уже есть: сервисы доверяют этому CA, перевыпуск только с --force",
                ca_pem.display()
            ),
        ));
    }
    fs::create_dir_all(dir)?;
    let mut ca = CertificateParams::new(Vec::new()).map_err(other)?;
    ca.distinguished_name
        .push(DnType::CommonName, "social network CA");
    ca.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate().map_err(other)?;
    let ca_cert = ca.self_signed(&ca_key).map_err(other)?;
    let mut written = Vec::new();
    write(&ca_pem, &ca_cert.pem(), false)?;
    written.push(ca_pem);
    let ca_secret = dir.join(format!("{CA}.key"));
    write(&ca_secret, &ca_key.serialize_pem(), true)?;
    written.push(ca_secret);
    for name in SERVICES {
        let names = vec![
            name.to_string(),
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ];
        let mut leaf = CertificateParams::new(names).map_err(other)?;
        leaf.distinguished_name.push(DnType::CommonName, name);
        leaf.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let key = KeyPair::generate().map_err(other)?;
        let cert = leaf.signed_by(&key, &ca_cert, &ca_key).map_err(other)?;
        let cert_path = dir.join(format!("{name}.pem"));
        let key_path = dir.join(format!("{name}.key"));
        write(&cert_path, &cert.pem(), false)?;
        write(&key_path, &key.serialize_pem(), true)?;
        written.push(cert_path);
        written.push(key_path);
    }
    Ok(written)
}
