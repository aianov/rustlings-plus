// Утилита выпуска сертификатов для сервисов: свой CA и по сертификату на сервис.
// Файлы с сертификатами в репозиторий не попадают: .gitignore должен их исключать.
use rcgen::{BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose};
use std::io::Write;
use std::path::Path;

pub const SERVICES: [&str; 3] = ["gateway", "posts", "users"];

fn write_file(path: &Path, text: &str, private: bool) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

fn other(e: rcgen::Error) -> std::io::Error {
    std::io::Error::other(e)
}

pub fn issue_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut ca_params = CertificateParams::new(Vec::new()).map_err(other)?;
    ca_params.distinguished_name.push(DnType::CommonName, "social CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate().map_err(other)?;
    let ca_cert = ca_params.self_signed(&ca_key).map_err(other)?;
    write_file(&dir.join("ca.pem"), &ca_cert.pem(), false)?;
    write_file(&dir.join("ca.key"), &ca_key.serialize_pem(), true)?;
    for name in SERVICES {
        let names = vec![format!("{name}.social.test"), "localhost".to_string(), "127.0.0.1".to_string()];
        let mut params = CertificateParams::new(names).map_err(other)?;
        params.extended_key_usages = if name == "posts" {
            vec![ExtendedKeyUsagePurpose::ServerAuth, ExtendedKeyUsagePurpose::ClientAuth]
        } else {
            vec![ExtendedKeyUsagePurpose::ClientAuth, ExtendedKeyUsagePurpose::ServerAuth]
        };
        let key = KeyPair::generate().map_err(other)?;
        let cert = params.signed_by(&key, &ca_cert, &ca_key).map_err(other)?;
        write_file(&dir.join(format!("{name}.pem")), &cert.pem(), false)?;
        write_file(&dir.join(format!("{name}.key")), &key.serialize_pem(), true)?;
    }
    Ok(())
}

pub fn ensure_gitignore(repo_root: &Path) -> std::io::Result<()> {
    let path = repo_root.join(".gitignore");
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if current.lines().any(|l| l.trim() == "certs/") {
        return Ok(());
    }
    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str("certs/\n");
    std::fs::write(path, next)
}
