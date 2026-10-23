pub mod base64;
pub mod hmac;
mod keys;

use crate::common::{claims_from, header_from, to_json};

pub use crate::common::{Claims, Header};
pub use keys::{KeyRing, MIN_KEY, SigningKey};

pub const ALG: &str = "HS256";

pub const ISSUER: &str = "auth-service";

pub const AUDIENCE: &str = "social";

pub const LEEWAY: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPolicy {
    pub issuer: String,
    pub audience: String,
    pub leeway: u64,
}

impl Default for TokenPolicy {
    fn default() -> TokenPolicy {
        TokenPolicy {
            issuer: ISSUER.to_string(),
            audience: AUDIENCE.to_string(),
            leeway: LEEWAY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn sign(keys: &KeyRing, claims: &Claims) -> String {
    let key = keys.current();
    let header = Header {
        alg: ALG.to_string(),
        kid: key.kid.clone(),
    };
    let input = format!(
        "{}.{}",
        base64::encode(&to_json(&header)),
        base64::encode(&to_json(claims))
    );
    let signature = hmac::hmac_sha256(key.key.expose(), input.as_bytes());
    format!("{input}.{}", base64::encode(&signature))
}

fn decoded(encoded: &str) -> Result<Vec<u8>, VerifyError> {
    base64::decode(encoded).ok_or(VerifyError::Malformed)
}

pub fn verify(
    keys: &KeyRing,
    token: &str,
    policy: &TokenPolicy,
    now: u64,
) -> Result<Claims, VerifyError> {
    let mut parts = token.split('.');
    let (Some(head), Some(body), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(VerifyError::Malformed);
    };
    let header = header_from(&decoded(head)?).ok_or(VerifyError::Malformed)?;
    if header.alg != ALG {
        return Err(VerifyError::UnsupportedAlg);
    }
    let key = keys.find(&header.kid).ok_or(VerifyError::UnknownKey)?;
    let signature = base64::decode(signature).ok_or(VerifyError::Malformed)?;
    let expected = hmac::hmac_sha256(key.key.expose(), format!("{head}.{body}").as_bytes());
    if !hmac::same(&expected, &signature) {
        return Err(VerifyError::BadSignature);
    }
    let claims = claims_from(&decoded(body)?).ok_or(VerifyError::Malformed)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifetimes {
    pub access: u64,
    pub refresh: u64,
    pub confirm: u64,
    pub reset: u64,
    pub leeway: u64,
}

impl Default for Lifetimes {
    fn default() -> Lifetimes {
        Lifetimes {
            access: 15 * 60,
            refresh: 30 * 24 * 60 * 60,
            confirm: 24 * 60 * 60,
            reset: 60 * 60,
            leeway: LEEWAY,
        }
    }
}
