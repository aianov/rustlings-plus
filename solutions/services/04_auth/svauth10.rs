
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

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    Malformed,
    UnsupportedAlg,
    BadSignature,
    Expired,
}

pub fn verify(token: &str, key: &[u8], now: u64) -> Result<Claims, VerifyError> {
    let parsed = parse(token).map_err(|_| VerifyError::Malformed)?;
    if parsed.header.alg != "HS256" {
        return Err(VerifyError::UnsupportedAlg);
    }
    let expected = hmac_sha256(key, parsed.signing_input.as_bytes());
    if !ct_eq(&expected, &parsed.signature) {
        return Err(VerifyError::BadSignature);
    }
    if parsed.claims.exp <= now {
        return Err(VerifyError::Expired);
    }
    Ok(parsed.claims)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"signing-key";

    fn claims(exp: u64) -> Claims {
        Claims {
            sub: "u-1".to_string(),
            iss: "auth.social".to_string(),
            aud: "social-api".to_string(),
            exp,
            nbf: 0,
            jti: "j".to_string(),
            role: "user".to_string(),
        }
    }

    fn custom(header_json: &str, claims: &Claims, signature: Option<&[u8]>) -> String {
        let input = format!("{}.{}", b64_encode(header_json.as_bytes()), b64_encode(&serde_json::to_vec(claims).unwrap()));
        let sig = match signature {
            Some(bytes) => b64_encode(bytes),
            None => b64_encode(&hmac_sha256(KEY, input.as_bytes())),
        };
        format!("{input}.{sig}")
    }

    #[test]
    fn honest_token_passes() {
        let token = issue("k1", KEY, &claims(2_000));
        assert_eq!(verify(&token, KEY, 1_000).map(|c| c.sub), Ok("u-1".to_string()), "обычный токен");
        assert_eq!(verify(&token, KEY, 2_000), Err(VerifyError::Expired), "срок вышел");
    }

    #[test]
    fn alg_none_is_never_accepted() {
        let token = custom(r#"{"alg":"none","kid":"k1"}"#, &claims(2_000), Some(&[]));
        assert_eq!(verify(&token, KEY, 1_000), Err(VerifyError::UnsupportedAlg), "алгоритм «none» значит «подписи нет», и это отказ");
    }

    #[test]
    fn other_algorithm_spellings_are_rejected_even_with_a_valid_hmac() {
        for alg in ["HS512", "hs256", "HS256 ", "RS256", ""] {
            let token = custom(&format!(r#"{{"alg":"{alg}","kid":"k1"}}"#), &claims(2_000), None);
            assert_eq!(verify(&token, KEY, 1_000), Err(VerifyError::UnsupportedAlg), "алгоритм {alg:?} мы не поддерживаем: выбор алгоритма не дело клиента");
        }
    }

    #[test]
    fn signature_is_checked_before_anything_inside() {
        let forged = custom(r#"{"alg":"HS256","kid":"k1"}"#, &claims(10), Some(&[1; 32]));
        assert_eq!(verify(&forged, KEY, 1_000), Err(VerifyError::BadSignature), "подделка с просроченными данными: сначала подпись");
        let empty_sig = custom(r#"{"alg":"HS256","kid":"k1"}"#, &claims(2_000), Some(&[]));
        assert_eq!(verify(&empty_sig, KEY, 1_000), Err(VerifyError::BadSignature), "пустая подпись у HS256 это неверная подпись");
    }

    #[test]
    fn one_flipped_bit_in_the_signature_fails() {
        let token = issue("k1", KEY, &claims(2_000));
        let parsed = parse(&token).unwrap();
        let mut sig = parsed.signature.clone();
        sig[31] ^= 1;
        let broken = format!("{}.{}", parsed.signing_input, b64_encode(&sig));
        assert_eq!(verify(&broken, KEY, 1_000), Err(VerifyError::BadSignature), "последний бит подписи");
    }
}
