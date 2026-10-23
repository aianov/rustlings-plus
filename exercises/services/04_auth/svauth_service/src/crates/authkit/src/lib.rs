use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            let v = ALPHABET.iter().position(|a| a == c)? as u32;
            n |= v << (18 - 6 * i);
        }
        let bytes = chunk.len() * 6 / 8;
        if n & ((1u32 << (24 - 8 * bytes)) - 1) != 0 {
            return None;
        }
        out.extend_from_slice(&n.to_be_bytes()[1..1 + bytes]);
    }
    Some(out)
}

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

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub alg: String,
    pub kid: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub exp: u64,
    pub nbf: u64,
    pub jti: String,
    pub role: String,
}

#[derive(Debug, PartialEq)]
pub enum TokenError {
    Malformed,
}

#[derive(Debug)]
pub struct Parsed {
    pub header: Header,
    pub claims: Claims,
    pub signing_input: String,
    pub signature: Vec<u8>,
}

pub fn issue(kid: &str, key: &[u8], claims: &Claims) -> String {
    let header = Header { alg: "HS256".to_string(), kid: kid.to_string() };
    let head = b64_encode(&serde_json::to_vec(&header).expect("заголовок"));
    let body = b64_encode(&serde_json::to_vec(claims).expect("claims"));
    let input = format!("{head}.{body}");
    let signature = b64_encode(&hmac_sha256(key, input.as_bytes()));
    format!("{input}.{signature}")
}

pub fn parse(token: &str) -> Result<Parsed, TokenError> {
    let parts: Vec<&str> = token.split('.').collect();
    let [head, body, signature] = parts[..] else {
        return Err(TokenError::Malformed);
    };
    let header = serde_json::from_slice(&b64_decode(head).ok_or(TokenError::Malformed)?).map_err(|_| TokenError::Malformed)?;
    let claims = serde_json::from_slice(&b64_decode(body).ok_or(TokenError::Malformed)?).map_err(|_| TokenError::Malformed)?;
    let signature = b64_decode(signature).ok_or(TokenError::Malformed)?;
    Ok(Parsed { header, claims, signing_input: format!("{head}.{body}"), signature })
}

pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    pub fn new(start: u64) -> ManualClock {
        ManualClock(std::sync::atomic::AtomicU64::new(start))
    }

    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

pub fn sha256_hex(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

pub struct KeySet {
    current: (String, Vec<u8>),
    previous: Vec<(String, Vec<u8>)>,
}

impl KeySet {
    pub fn new(kid: &str, key: &[u8]) -> KeySet {
        KeySet { current: (kid.to_string(), key.to_vec()), previous: Vec::new() }
    }

    pub fn rotate(&mut self, kid: &str, key: &[u8]) {
        let old = std::mem::replace(&mut self.current, (kid.to_string(), key.to_vec()));
        self.previous.push(old);
    }

    pub fn retire(&mut self, kid: &str) -> bool {
        let before = self.previous.len();
        self.previous.retain(|(k, _)| k != kid);
        self.previous.len() != before
    }

    pub fn current(&self) -> (&str, &[u8]) {
        (&self.current.0, &self.current.1)
    }

    fn find(&self, kid: &str) -> Option<&[u8]> {
        std::iter::once(&self.current).chain(self.previous.iter()).find(|(k, _)| k == kid).map(|(_, key)| key.as_slice())
    }
}

pub fn issue_current(keys: &KeySet, claims: &Claims) -> String {
    let (kid, key) = keys.current();
    issue(kid, key, claims)
}

pub struct Policy {
    pub issuer: String,
    pub audience: String,
    pub leeway: u64,
}

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    Malformed,
    UnsupportedAlg,
    UnknownKey,
    BadSignature,
    Expired,
    NotYetValid,
    WrongIssuer,
    WrongAudience,
}

pub fn verify(keys: &KeySet, token: &str, policy: &Policy, clock: &dyn Clock) -> Result<Claims, VerifyError> {
    let parsed = parse(token).map_err(|_| VerifyError::Malformed)?;
    if parsed.header.alg != "HS256" {
        return Err(VerifyError::UnsupportedAlg);
    }
    let key = keys.find(&parsed.header.kid).ok_or(VerifyError::UnknownKey)?;
    if !ct_eq(&hmac_sha256(key, parsed.signing_input.as_bytes()), &parsed.signature) {
        return Err(VerifyError::BadSignature);
    }
    let now = clock.now();
    let claims = parsed.claims;
    if now >= claims.exp.saturating_add(policy.leeway) {
        return Err(VerifyError::Expired);
    }
    if now.saturating_add(policy.leeway) < claims.nbf {
        return Err(VerifyError::NotYetValid);
    }
    if claims.iss != policy.issuer {
        return Err(VerifyError::WrongIssuer);
    }
    if claims.aud != policy.audience {
        return Err(VerifyError::WrongAudience);
    }
    Ok(claims)
}
