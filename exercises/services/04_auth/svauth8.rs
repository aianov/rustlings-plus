// Свой токен выглядит как `заголовок.данные.подпись`, где каждая часть это base64url. В заголовке
// алгоритм и идентификатор ключа `kid`, в данных кто, для кого, до какого срока и с какой ролью
// (claims), подпись это HMAC-SHA256 от первых двух частей вместе с точкой. Цель: `issue` собирает
// токен, а `parse` разбирает его обратно в части (проверять подпись пока не нужно, это следующая задача).
// Мусор вместо токена это `Malformed`, а не паника.

use serde::{Deserialize, Serialize};

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
    todo!()
}

pub fn parse(token: &str) -> Result<Parsed, TokenError> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> Claims {
        Claims {
            sub: "u-1".to_string(),
            iss: "auth.social".to_string(),
            aud: "social-api".to_string(),
            exp: 2_000,
            nbf: 1_000,
            jti: "j-1".to_string(),
            role: "user".to_string(),
        }
    }

    #[test]
    fn token_has_three_url_safe_parts() {
        let token = issue("k1", b"secret", &claims());
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "заголовок, данные и подпись через точки: {token}");
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)), "только символы base64url и точки: {token}");
    }

    #[test]
    fn header_names_the_algorithm_and_the_key() {
        let token = issue("k-2024", b"secret", &claims());
        let header = token.split('.').next().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&b64_decode(header).expect("base64url")).expect("JSON");
        assert_eq!(json["alg"], "HS256", "алгоритм");
        assert_eq!(json["kid"], "k-2024", "идентификатор ключа");
    }

    #[test]
    fn signature_covers_header_dot_claims() {
        let token = issue("k1", b"secret", &claims());
        let (input, signature) = token.rsplit_once('.').unwrap();
        assert_eq!(b64_decode(signature).unwrap(), hmac_sha256(b"secret", input.as_bytes()).to_vec(), "подпись это HMAC от `заголовок.данные`");
    }

    #[test]
    fn parse_gives_back_the_parts() {
        let token = issue("k1", b"secret", &claims());
        let parsed = parse(&token).expect("свой токен разбирается");
        assert_eq!(parsed.header, Header { alg: "HS256".to_string(), kid: "k1".to_string() }, "заголовок");
        assert_eq!(parsed.claims, claims(), "claims");
        assert_eq!(parsed.signature.len(), 32, "подпись SHA-256 это 32 байта");
        assert_eq!(parsed.signing_input, token.rsplit_once('.').unwrap().0, "то, что подписано, возвращается как есть: ровно эти байты проверяются");
    }

    #[test]
    fn garbage_is_malformed() {
        let good = issue("k1", b"s", &claims());
        let mut cases = vec![
            "".to_string(),
            "abc".to_string(),
            "a.b".to_string(),
            format!("{good}.extra"),
            "!!!.@@@.###".to_string(),
        ];
        let (head, rest) = good.split_once('.').unwrap();
        cases.push(format!("{}.{rest}", b64_encode(b"not json")));
        cases.push(format!("{head}.{}.{}", b64_encode(b"{\"sub\":1}"), "AAAA"));
        for case in cases {
            assert_eq!(parse(&case).unwrap_err(), TokenError::Malformed, "{case:?} это не токен");
        }
    }
}
