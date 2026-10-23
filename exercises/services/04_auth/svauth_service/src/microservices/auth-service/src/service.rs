use crate::dto::{Credentials, NewUser};
use crate::repository::Store;
use argon2::Params;
use authkit::{Clock, KeySet};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

pub const ACCESS_TTL: u64 = 900;
pub const REFRESH_TTL: u64 = 30 * 24 * 3600;
pub const ISSUER: &str = "auth.social";
pub const AUDIENCE: &str = "social-api";
pub const LEEWAY: u64 = 30;

pub struct Config {
    pub clock: Arc<dyn Clock>,
    pub keys: KeySet,
    pub params: Params,
    pub max_parallel: usize,
    pub probe: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    pub user_id: String,
    pub role: String,
    pub jti: String,
}

#[derive(Debug, PartialEq)]
pub enum ValidateError {
    Invalid,
    Expired,
    Revoked,
}

#[derive(Debug, PartialEq)]
pub enum ServiceError {
    UsernameTaken,
    InvalidCredentials,
    InvalidRefresh,
    Unauthenticated(ValidateError),
}

pub struct AuthService {
    clock: Arc<dyn Clock>,
    keys: Mutex<KeySet>,
    params: Params,
    permits: Arc<Semaphore>,
    probe: Arc<dyn Fn() + Send + Sync>,
    store: Store,
    dummy_hash: String,
}

impl AuthService {
    pub fn new(config: Config) -> AuthService {
        todo!()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn rotate_key(&self, kid: &str, key: &[u8]) {
        todo!()
    }

    pub async fn register(&self, new: NewUser) -> Result<Account, ServiceError> {
        todo!()
    }

    pub async fn login(&self, credentials: Credentials) -> Result<Tokens, ServiceError> {
        todo!()
    }

    pub fn refresh(&self, token: &str) -> Result<Tokens, ServiceError> {
        todo!()
    }

    pub fn validate(&self, token: &str) -> Result<Principal, ValidateError> {
        todo!()
    }

    pub fn logout(&self, access: &str) -> Result<(), ServiceError> {
        todo!()
    }

    pub fn logout_all(&self, access: &str) -> Result<(), ServiceError> {
        todo!()
    }

    pub fn account(&self, id: &str) -> Option<Account> {
        todo!()
    }
}
