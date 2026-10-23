use std::time::Duration;

use files::{Kind, LinkError, Linked, Signer};

use super::unix_now;

pub const LINK_KEY_ENV: &str = "GATEWAY_LINK_KEY";

pub const DEFAULT_LINK_KEY: &str = "dev-link-key-change-me";

pub const LINK_TTL_ENV: &str = "GATEWAY_LINK_TTL_MS";

pub const LINK_TTL: Duration = Duration::from_secs(3600);

pub const UPLOAD_BUDGET_ENV: &str = "GATEWAY_UPLOAD_BUDGET_MS";

pub const UPLOAD_BUDGET: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Links {
    signer: Signer,
    ttl: Duration,
}

impl Links {
    pub fn new(key: &str, ttl: Duration) -> Links {
        Links {
            signer: Signer::new(key.as_bytes()),
            ttl,
        }
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn file(&self, key: &str, name: &str) -> String {
        let exp = unix_now() + self.ttl.as_secs().max(1);
        self.signer.link(key, exp, name)
    }

    pub fn avatar(&self, key: &str, content_type: &str) -> String {
        let ext = Kind::parse(content_type).map_or("bin", Kind::ext);
        self.file(key, &format!("avatar.{ext}"))
    }

    pub fn verify(&self, target: &str) -> Result<Linked, LinkError> {
        self.signer.verify(target, unix_now())
    }
}

impl Default for Links {
    fn default() -> Links {
        Links::new(DEFAULT_LINK_KEY, LINK_TTL)
    }
}
