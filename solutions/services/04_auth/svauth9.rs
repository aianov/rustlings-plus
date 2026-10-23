
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

pub trait Clock {
    fn now(&self) -> u64;
}

pub struct Policy {
    pub issuer: String,
    pub audience: String,
    pub leeway: u64,
}

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    Malformed,
    BadSignature,
    Expired,
    NotYetValid,
    WrongIssuer,
    WrongAudience,
}

pub fn verify(token: &str, key: &[u8], policy: &Policy, clock: &dyn Clock) -> Result<Claims, VerifyError> {
    let parsed = parse(token).map_err(|_| VerifyError::Malformed)?;
    let expected = hmac_sha256(key, parsed.signing_input.as_bytes());
    if !ct_eq(&expected, &parsed.signature) {
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    struct At(u64);

    impl Clock for At {
        fn now(&self) -> u64 {
            self.0
        }
    }

    const KEY: &[u8] = b"signing-key";

    fn policy() -> Policy {
        Policy { issuer: "auth.social".to_string(), audience: "social-api".to_string(), leeway: 30 }
    }

    fn claims() -> Claims {
        Claims {
            sub: "u-1".to_string(),
            iss: "auth.social".to_string(),
            aud: "social-api".to_string(),
            exp: 2_000,
            nbf: 1_000,
            jti: "j".to_string(),
            role: "user".to_string(),
        }
    }

    fn check(claims: &Claims, now: u64) -> Result<Claims, VerifyError> {
        verify(&issue("k1", KEY, claims), KEY, &policy(), &At(now))
    }

    #[test]
    fn good_token_inside_its_window() {
        assert_eq!(check(&claims(), 1_500), Ok(claims()), "обычный случай");
    }

    #[test]
    fn expiry_edge_with_leeway() {
        assert!(check(&claims(), 2_029).is_ok(), "exp + leeway - 1 ещё можно");
        assert_eq!(check(&claims(), 2_030), Err(VerifyError::Expired), "now >= exp + leeway это конец");
    }

    #[test]
    fn not_before_edge_with_leeway() {
        assert!(check(&claims(), 970).is_ok(), "now + leeway == nbf уже можно");
        assert_eq!(check(&claims(), 969), Err(VerifyError::NotYetValid), "ещё рано");
    }

    #[test]
    fn issuer_and_audience_are_checked() {
        let other_issuer = Claims { iss: "evil".to_string(), ..claims() };
        assert_eq!(check(&other_issuer, 1_500), Err(VerifyError::WrongIssuer), "чужой издатель");
        let other_audience = Claims { aud: "billing".to_string(), ..claims() };
        assert_eq!(check(&other_audience, 1_500), Err(VerifyError::WrongAudience), "токен для другого сервиса");
    }

    #[test]
    fn signature_comes_first() {
        let expired = Claims { exp: 10, ..claims() };
        let token = issue("k1", b"other-key", &expired);
        assert_eq!(
            verify(&token, KEY, &policy(), &At(1_500)),
            Err(VerifyError::BadSignature),
            "чужая подпись и просроченный срок: честный ответ «подпись неверна», потому что срок в таком токене писал кто угодно"
        );
    }

    #[test]
    fn tampered_claims_do_not_pass() {
        let token = issue("k1", KEY, &claims());
        let parts: Vec<&str> = token.split('.').collect();
        let forged = Claims { role: "admin".to_string(), ..claims() };
        let forged_body = b64_encode(&serde_json::to_vec(&forged).unwrap());
        let tampered = format!("{}.{}.{}", parts[0], forged_body, parts[2]);
        assert_eq!(verify(&tampered, KEY, &policy(), &At(1_500)), Err(VerifyError::BadSignature), "подмена роли без нового ключа");
    }

    #[test]
    fn order_among_claim_checks() {
        let both = Claims { exp: 10, iss: "evil".to_string(), ..claims() };
        assert_eq!(check(&both, 1_500), Err(VerifyError::Expired), "срок проверяется раньше издателя");
        let early_and_foreign = Claims { nbf: 5_000, aud: "x".to_string(), ..claims() };
        assert_eq!(check(&early_and_foreign, 1_500), Err(VerifyError::NotYetValid), "nbf раньше аудитории");
    }

    #[test]
    fn garbage_is_malformed() {
        assert_eq!(verify("a.b", KEY, &policy(), &At(0)), Err(VerifyError::Malformed), "не токен");
    }
}
