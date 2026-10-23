use crate::dto::{Credentials, NewUser};
use crate::repository::{Inner, SessionRow, Store, User};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use authkit::{Claims, Clock, KeySet, Policy, VerifyError, issue_current, random_token, sha256_hex, verify};
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

fn argon(params: &Params) -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone())
}

fn hash_now(params: &Params, password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    argon(params).hash_password(password.as_bytes(), &salt).expect("хеширование").to_string()
}

fn revoke_family(db: &mut Inner, family: u64) {
    db.revoked_families.insert(family);
    let denied: Vec<(String, u64)> = db.sessions.values().filter(|s| s.family == family).map(|s| (s.access_jti.clone(), s.access_exp)).collect();
    db.denied.extend(denied);
}

impl AuthService {
    pub fn new(config: Config) -> AuthService {
        let dummy_hash = hash_now(&config.params, &random_token());
        AuthService {
            clock: config.clock,
            keys: Mutex::new(config.keys),
            params: config.params,
            permits: Arc::new(Semaphore::new(config.max_parallel)),
            probe: config.probe,
            store: Store::new(),
            dummy_hash,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn rotate_key(&self, kid: &str, key: &[u8]) {
        self.keys.lock().unwrap_or_else(|p| p.into_inner()).rotate(kid, key);
    }

    async fn hash(&self, password: &str) -> String {
        let _permit = self.permits.acquire().await.expect("семафор не закрывается");
        let (params, probe, password) = (self.params.clone(), self.probe.clone(), password.to_string());
        tokio::task::spawn_blocking(move || {
            probe();
            hash_now(&params, &password)
        })
        .await
        .expect("задача хеширования")
    }

    async fn check(&self, password: &str, stored: &str) -> bool {
        let _permit = self.permits.acquire().await.expect("семафор не закрывается");
        let (params, probe) = (self.params.clone(), self.probe.clone());
        let (password, stored) = (password.to_string(), stored.to_string());
        tokio::task::spawn_blocking(move || {
            probe();
            match PasswordHash::new(&stored) {
                Ok(parsed) => argon(&params).verify_password(password.as_bytes(), &parsed).is_ok(),
                Err(_) => false,
            }
        })
        .await
        .unwrap_or(false)
    }

    pub async fn register(&self, new: NewUser) -> Result<Account, ServiceError> {
        let password_hash = self.hash(&new.password).await;
        self.store.transaction(|db| {
            if db.by_name.contains_key(&new.username) {
                return Err(ServiceError::UsernameTaken);
            }
            db.next_user += 1;
            let id = format!("u-{}", db.next_user);
            let user = User { id: id.clone(), username: new.username.clone(), email: new.email, password_hash, role: "user".to_string() };
            let account = Account { id: id.clone(), username: user.username.clone(), email: user.email.clone(), role: user.role.clone() };
            db.by_name.insert(new.username, id.clone());
            db.users.insert(id, user);
            Ok(account)
        })
    }

    pub async fn login(&self, credentials: Credentials) -> Result<Tokens, ServiceError> {
        let user = self.store.transaction(|db| db.by_name.get(&credentials.username).and_then(|id| db.users.get(id)).cloned());
        let stored = user.as_ref().map(|u| u.password_hash.as_str()).unwrap_or(&self.dummy_hash);
        let matches = self.check(&credentials.password, stored).await;
        match user {
            Some(user) if matches => Ok(self.store.transaction(|db| {
                db.next_family += 1;
                let family = db.next_family;
                self.open(db, &user, family)
            })),
            _ => Err(ServiceError::InvalidCredentials),
        }
    }

    fn open(&self, db: &mut Inner, user: &User, family: u64) -> Tokens {
        let now = self.clock.now();
        let claims = Claims {
            sub: user.id.clone(),
            iss: ISSUER.to_string(),
            aud: AUDIENCE.to_string(),
            exp: now + ACCESS_TTL,
            nbf: now,
            jti: random_token(),
            role: user.role.clone(),
        };
        let access = issue_current(&self.keys.lock().unwrap_or_else(|p| p.into_inner()), &claims);
        let refresh = random_token();
        db.sessions.insert(
            sha256_hex(&refresh),
            SessionRow { family, user_id: user.id.clone(), used: false, expires: now + REFRESH_TTL, access_jti: claims.jti, access_exp: claims.exp },
        );
        Tokens { access, refresh, expires_in: ACCESS_TTL }
    }

    pub fn refresh(&self, token: &str) -> Result<Tokens, ServiceError> {
        let key = sha256_hex(token);
        let now = self.clock.now();
        self.store.transaction(|db| {
            let row = db.sessions.get(&key).cloned().ok_or(ServiceError::InvalidRefresh)?;
            if db.revoked_families.contains(&row.family) {
                return Err(ServiceError::InvalidRefresh);
            }
            if row.used {
                revoke_family(db, row.family);
                return Err(ServiceError::InvalidRefresh);
            }
            if row.expires <= now {
                return Err(ServiceError::InvalidRefresh);
            }
            let user = db.users.get(&row.user_id).cloned().ok_or(ServiceError::InvalidRefresh)?;
            if let Some(stored) = db.sessions.get_mut(&key) {
                stored.used = true;
            }
            Ok(self.open(db, &user, row.family))
        })
    }

    pub fn validate(&self, token: &str) -> Result<Principal, ValidateError> {
        let policy = Policy { issuer: ISSUER.to_string(), audience: AUDIENCE.to_string(), leeway: LEEWAY };
        let claims = {
            let keys = self.keys.lock().unwrap_or_else(|p| p.into_inner());
            verify(&keys, token, &policy, self.clock.as_ref()).map_err(|e| match e {
                VerifyError::Expired => ValidateError::Expired,
                _ => ValidateError::Invalid,
            })?
        };
        if self.store.transaction(|db| db.denied.contains_key(&claims.jti)) {
            return Err(ValidateError::Revoked);
        }
        Ok(Principal { user_id: claims.sub, role: claims.role, jti: claims.jti })
    }

    pub fn logout(&self, access: &str) -> Result<(), ServiceError> {
        let principal = self.validate(access).map_err(ServiceError::Unauthenticated)?;
        self.store.transaction(|db| {
            let families: Vec<u64> = db.sessions.values().filter(|s| s.access_jti == principal.jti).map(|s| s.family).collect();
            for family in families {
                revoke_family(db, family);
            }
            db.denied.insert(principal.jti.clone(), self.clock.now() + ACCESS_TTL + LEEWAY);
        });
        Ok(())
    }

    pub fn logout_all(&self, access: &str) -> Result<(), ServiceError> {
        let principal = self.validate(access).map_err(ServiceError::Unauthenticated)?;
        self.store.transaction(|db| {
            let families: std::collections::HashSet<u64> = db.sessions.values().filter(|s| s.user_id == principal.user_id).map(|s| s.family).collect();
            for family in families {
                revoke_family(db, family);
            }
        });
        Ok(())
    }

    pub fn account(&self, id: &str) -> Option<Account> {
        self.store.transaction(|db| {
            db.users.get(id).map(|u| Account { id: u.id.clone(), username: u.username.clone(), email: u.email.clone(), role: u.role.clone() })
        })
    }
}
