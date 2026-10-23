use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use certgen::{CA, SERVICES, ensure_gitignore, issue_all};
use x509_parser::extensions::GeneralName;
use x509_parser::pem::parse_x509_pem;

fn sandbox(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m32-certgen")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("папка теста создаётся");
    dir
}

fn names(pem: &[u8]) -> (Vec<String>, Vec<IpAddr>, String, String) {
    let (_, pem) = parse_x509_pem(pem).expect("PEM разбирается");
    let cert = pem.parse_x509().expect("сертификат разбирается");
    let mut dns = Vec::new();
    let mut ips = Vec::new();
    if let Ok(Some(san)) = cert.subject_alternative_name() {
        for name in &san.value.general_names {
            match name {
                GeneralName::DNSName(name) => dns.push(name.to_string()),
                GeneralName::IPAddress(raw) if raw.len() == 4 => {
                    ips.push(IpAddr::V4(Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3])));
                }
                _ => {}
            }
        }
    }
    (
        dns,
        ips,
        cert.subject().to_string(),
        cert.issuer().to_string(),
    )
}

#[test]
fn ca_and_every_service_get_certificates() {
    let dir = sandbox("issue").join("certs");
    let written = issue_all(&dir, false).expect("выпуск проходит");
    assert_eq!(
        written.len(),
        2 + 2 * SERVICES.len(),
        "CA и по паре файлов на сервис"
    );
    let ca = fs::read(dir.join(format!("{CA}.pem"))).expect("ca.pem");
    let (_, _, ca_subject, ca_issuer) = names(&ca);
    assert_eq!(ca_subject, ca_issuer, "CA подписан сам собой");
    for service in SERVICES {
        let pem = fs::read(dir.join(format!("{service}.pem"))).expect("сертификат сервиса");
        let (dns, ips, _, issuer) = names(&pem);
        assert!(
            dns.contains(&service.to_string()) && dns.contains(&"localhost".to_string()),
            "{service}: имя сервиса и localhost в SAN, по ним клиенты проверяют сервер: {dns:?}"
        );
        assert_eq!(
            ips,
            [IpAddr::V4(Ipv4Addr::LOCALHOST)],
            "{service}: и адрес 127.0.0.1"
        );
        assert_eq!(issuer, ca_subject, "{service}: подписан нашим CA");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key = fs::metadata(dir.join(format!("{service}.key"))).expect("ключ");
            assert_eq!(
                key.permissions().mode() & 0o777,
                0o600,
                "{service}: ключ читает только владелец"
            );
        }
    }
}

#[test]
fn existing_ca_is_kept_unless_forced() {
    let dir = sandbox("again").join("certs");
    issue_all(&dir, false).expect("первый выпуск");
    let before = fs::read(dir.join("ca.pem")).expect("ca.pem");
    let again = issue_all(&dir, false).expect_err("второй выпуск без --force");
    assert_eq!(again.kind(), ErrorKind::AlreadyExists, "{again}");
    assert_eq!(
        fs::read(dir.join("ca.pem")).expect("ca.pem"),
        before,
        "CA не перезаписан: сервисы ему доверяют"
    );
    issue_all(&dir, true).expect("с force перевыпускается");
    assert_ne!(
        fs::read(dir.join("ca.pem")).expect("ca.pem"),
        before,
        "новый CA"
    );
}

#[test]
fn gitignore_gets_the_line_once() {
    let root = sandbox("gitignore");
    fs::write(root.join(".gitignore"), "/target").expect("старый .gitignore");
    assert!(ensure_gitignore(&root, "certs/").expect("дописывается"));
    assert!(!ensure_gitignore(&root, "certs/").expect("второй раз ничего"));
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect(".gitignore"),
        "/target\ncerts/\n"
    );
}
