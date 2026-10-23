use sha2::{Digest, Sha256};

use crate::content::hex;
use crate::store::valid_key;

const BLOCK: usize = 64;

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    Format,
    Signature,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub key: String,
    pub name: String,
    pub exp: u64,
}

#[derive(Clone)]
pub struct Signer {
    key: Vec<u8>,
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

impl Signer {
    pub fn new(key: &[u8]) -> Signer {
        Signer { key: key.to_vec() }
    }

    fn sign(&self, key: &str, exp: u64, name: &str) -> String {
        hex(&hmac_sha256(
            &self.key,
            format!("{key}\n{exp}\n{name}").as_bytes(),
        ))
    }

    pub fn link(&self, key: &str, exp: u64, name: &str) -> String {
        let sig = self.sign(key, exp, name);
        format!("/files/{key}?exp={exp}&name={name}&sig={sig}")
    }

    pub fn verify(&self, path_and_query: &str, now: u64) -> Result<Linked, LinkError> {
        let (path, query) = path_and_query.split_once('?').ok_or(LinkError::Format)?;
        let key = path.strip_prefix("/files/").ok_or(LinkError::Format)?;
        let (mut exp, mut name, mut sig) = (None, None, None);
        for pair in query.split('&') {
            let (field, value) = pair.split_once('=').ok_or(LinkError::Format)?;
            let slot = match field {
                "exp" => &mut exp,
                "name" => &mut name,
                "sig" => &mut sig,
                _ => return Err(LinkError::Format),
            };
            if slot.replace(value).is_some() {
                return Err(LinkError::Format);
            }
        }
        let (Some(exp), Some(name), Some(sig)) = (exp, name, sig) else {
            return Err(LinkError::Format);
        };
        let exp: u64 = exp.parse().map_err(|_| LinkError::Format)?;
        if !valid_key(key) || !safe_name(name) {
            return Err(LinkError::Format);
        }
        if !same(self.sign(key, exp, name).as_bytes(), sig.as_bytes()) {
            return Err(LinkError::Signature);
        }
        if now >= exp {
            return Err(LinkError::Expired);
        }
        Ok(Linked {
            key: key.to_string(),
            name: name.to_string(),
            exp,
        })
    }
}
