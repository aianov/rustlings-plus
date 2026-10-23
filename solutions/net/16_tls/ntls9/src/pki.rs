#![allow(dead_code)]
use rcgen::{BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose};

pub struct Ca {
    cert: rcgen::Certificate,
    key: KeyPair,
}

pub struct Leaf {
    pub cert_pem: String,
    pub key_pem: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Server,
    Client,
}

pub type Day = (i32, u8, u8);

impl Ca {
    pub fn new(name: &str) -> Ca {
        let mut params = CertificateParams::new(Vec::new()).expect("параметры CA");
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key = KeyPair::generate().expect("ключ CA");
        let cert = params.self_signed(&key).expect("сертификат CA");
        Ca { cert, key }
    }

    pub fn cert_pem(&self) -> String {
        self.cert.pem()
    }

    pub fn issue(&self, names: &[&str], usage: Usage) -> Leaf {
        self.build(names, usage, None, None)
    }

    pub fn issue_valid(&self, names: &[&str], usage: Usage, from: Day, to: Day) -> Leaf {
        self.build(names, usage, Some((from, to)), None)
    }

    pub fn issue_cn(&self, common_name: &str, names: &[&str], usage: Usage) -> Leaf {
        self.build(names, usage, None, Some(common_name))
    }

    fn build(&self, names: &[&str], usage: Usage, days: Option<(Day, Day)>, cn: Option<&str>) -> Leaf {
        let mut params = CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>()).expect("параметры сертификата");
        if let Some(cn) = cn {
            params.distinguished_name.push(DnType::CommonName, cn);
        }
        params.extended_key_usages = vec![match usage {
            Usage::Server => ExtendedKeyUsagePurpose::ServerAuth,
            Usage::Client => ExtendedKeyUsagePurpose::ClientAuth,
        }];
        if let Some((from, to)) = days {
            params.not_before = rcgen::date_time_ymd(from.0, from.1, from.2);
            params.not_after = rcgen::date_time_ymd(to.0, to.1, to.2);
        }
        let key = KeyPair::generate().expect("ключ сертификата");
        let cert = params.signed_by(&key, &self.cert, &self.key).expect("подпись сертификата");
        Leaf { cert_pem: cert.pem(), key_pem: key.serialize_pem() }
    }
}
