// Свой удостоверяющий центр: сертификат CA, сертификат сервера и запись файлов с правильными правами.
// Здесь нет готовых проверок, только rcgen: допиши четыре функции.
#![allow(unused_variables)]

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
        todo!()
    }

    pub fn cert_pem(&self) -> String {
        todo!()
    }

    pub fn issue_server(&self, names: &[&str]) -> Result<Leaf, rcgen::Error> {
        todo!()
    }
}

pub fn write_pems(dir: &Path, ca: &Ca, leaf: &Leaf) -> std::io::Result<()> {
    todo!()
}
