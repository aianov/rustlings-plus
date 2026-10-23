use std::fmt;

use service_runtime::Secret;

use super::base64;

pub const MIN_KEY: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct SigningKey {
    pub kid: String,
    pub key: Secret<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct KeyRing {
    current: SigningKey,
    previous: Vec<SigningKey>,
}

impl fmt::Debug for KeyRing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let previous: Vec<&str> = self.previous.iter().map(|key| key.kid.as_str()).collect();
        f.debug_struct("KeyRing")
            .field("current", &self.current.kid)
            .field("previous", &previous)
            .finish()
    }
}

fn random_bytes() -> Vec<u8> {
    let mut bytes = vec![0u8; MIN_KEY];
    if getrandom::fill(&mut bytes).is_err() {
        bytes.clear();
    }
    bytes
}

impl KeyRing {
    pub fn new(kid: &str, key: Vec<u8>) -> KeyRing {
        KeyRing {
            current: SigningKey {
                kid: kid.to_string(),
                key: Secret::new(key),
            },
            previous: Vec::new(),
        }
    }

    pub fn random() -> Option<KeyRing> {
        let key = random_bytes();
        let kid = format!("k{}", super::hmac::hex(&key[..4.min(key.len())]));
        (key.len() == MIN_KEY).then(|| KeyRing::new(&kid, key))
    }

    pub fn parse(spec: &str) -> Result<KeyRing, String> {
        let mut keys = Vec::new();
        for item in spec
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let Some((kid, encoded)) = item.split_once(':') else {
                return Err("ждали kid:ключ_base64url через запятую".to_string());
            };
            let key =
                base64::decode(encoded.trim()).ok_or_else(|| format!("ключ {kid} не base64url"))?;
            if key.len() < MIN_KEY {
                return Err(format!("ключ {kid} короче {MIN_KEY} байт"));
            }
            keys.push(SigningKey {
                kid: kid.trim().to_string(),
                key: Secret::new(key),
            });
        }
        let mut keys = keys.into_iter();
        let current = keys
            .next()
            .ok_or_else(|| "нет ни одного ключа".to_string())?;
        Ok(KeyRing {
            current,
            previous: keys.collect(),
        })
    }

    pub fn rotate(&mut self, kid: &str, key: Vec<u8>) {
        let fresh = SigningKey {
            kid: kid.to_string(),
            key: Secret::new(key),
        };
        let old = std::mem::replace(&mut self.current, fresh);
        self.previous.insert(0, old);
    }

    pub fn retire(&mut self, kid: &str) -> bool {
        let before = self.previous.len();
        self.previous.retain(|key| key.kid != kid);
        before != self.previous.len()
    }

    pub fn current(&self) -> &SigningKey {
        &self.current
    }

    pub fn find(&self, kid: &str) -> Option<&SigningKey> {
        std::iter::once(&self.current)
            .chain(&self.previous)
            .find(|key| key.kid == kid)
    }
}
