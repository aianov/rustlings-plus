use sha2::{Digest, Sha256};

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner_hash);
    outer.finalize().into()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, PartialEq)]
pub enum LinkError {
    Malformed,
    BadSignature,
    Expired,
}

#[derive(Debug, PartialEq)]
pub struct Opened {
    pub key: String,
    pub name: String,
}

pub struct Signer {
    secret: Vec<u8>,
}

fn safe(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128 && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn mac(secret: &[u8], key: &str, exp: u64, name: &str) -> String {
    hex(&hmac_sha256(secret, format!("{key}\n{exp}\n{name}").as_bytes()))
}

impl Signer {
    pub fn new(secret: &[u8]) -> Signer {
        Signer { secret: secret.to_vec() }
    }

    pub fn link(&self, key: &str, expires_at: u64, name: &str) -> String {
        format!("/files/{key}?exp={expires_at}&name={name}&sig={}", mac(&self.secret, key, expires_at, name))
    }

    pub fn verify(&self, path_and_query: &str, now: u64) -> Result<Opened, LinkError> {
        let rest = path_and_query.strip_prefix("/files/").ok_or(LinkError::Malformed)?;
        let (key, query) = rest.split_once('?').ok_or(LinkError::Malformed)?;
        let query = query.strip_prefix("exp=").ok_or(LinkError::Malformed)?;
        let (exp, query) = query.split_once("&name=").ok_or(LinkError::Malformed)?;
        let (name, sig) = query.split_once("&sig=").ok_or(LinkError::Malformed)?;
        let exp: u64 = exp.parse().map_err(|_| LinkError::Malformed)?;
        if !safe(key) || !safe(name) || sig.len() != 64 || !sig.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(LinkError::Malformed);
        }
        if !ct_eq(sig.as_bytes(), mac(&self.secret, key, exp, name).as_bytes()) {
            return Err(LinkError::BadSignature);
        }
        if now >= exp {
            return Err(LinkError::Expired);
        }
        Ok(Opened { key: key.to_string(), name: name.to_string() })
    }
}
