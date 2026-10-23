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
        let _ = (key, expires_at, name, mac(&self.secret, key, expires_at, name), safe(key), ct_eq(&[], &[]));
        todo!()
    }

    pub fn verify(&self, path_and_query: &str, now: u64) -> Result<Opened, LinkError> {
        let _ = (path_and_query, now);
        todo!()
    }
}
