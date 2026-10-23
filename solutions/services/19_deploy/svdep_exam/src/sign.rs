use std::fmt::Write;

use sha2::{Digest, Sha256};

const BLOCK: usize = 64;

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

pub fn signature(key: &str, body: &[u8]) -> String {
    let mut out = String::from("sha256=");
    for byte in hmac_sha256(key.as_bytes(), body) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn hex_sha256(data: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(data) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
