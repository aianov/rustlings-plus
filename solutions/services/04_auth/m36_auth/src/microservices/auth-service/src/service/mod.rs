mod directory;
mod error;
mod exits;
mod letters;
mod mail;
pub mod password;
pub mod token;

use std::sync::{Arc, PoisonError, RwLock};

use contracts::{Clock, Role, Username};
use service_runtime::Secret;

use crate::common::{Account, Registration};
use crate::repository::{
    AccountRepository, MemoryAccounts, MemoryOneTime, MemorySessions, OneTimeRepository, Opening,
    RepoError, SessionRepository, SessionRow,
};

pub use directory::{ProfileDirectory, ProfileError};
pub use error::AuthError;
pub use mail::{Letter, Mailer};
pub use password::{Cost, PasswordError, Passwords, Verdict};
pub use token::{Claims, KeyRing, TokenPolicy, VerifyError};

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
            leeway: token::LEEWAY,
        }
    }
}

pub struct AuthParts {
    pub accounts: Arc<dyn AccountRepository>,
    pub sessions: Arc<dyn SessionRepository>,
    pub once: Arc<dyn OneTimeRepository>,
    pub passwords: Arc<Passwords>,
    pub keys: KeyRing,
    pub clock: Arc<dyn Clock>,
    pub mailer: Arc<dyn Mailer>,
    pub lifetimes: Lifetimes,
    pub moderators: Vec<String>,
    pub public_url: String,
}

impl AuthParts {
    pub fn memory(clock: Arc<dyn Clock>, mailer: Arc<dyn Mailer>) -> Result<AuthParts, AuthError> {
        Ok(AuthParts {
            accounts: Arc::new(MemoryAccounts::new()),
            sessions: Arc::new(MemorySessions::new(Lifetimes::default().leeway)),
            once: Arc::new(MemoryOneTime::default()),
            passwords: Arc::new(Passwords::new(Cost::QUICK, 4)?),
            keys: KeyRing::random().ok_or(AuthError::Random)?,
            clock,
            mailer,
            lifetimes: Lifetimes::default(),
            moderators: Vec::new(),
            public_url: String::from("https://auth.test"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub user_id: u64,
    pub username: String,
    pub role: Role,
    pub jti: String,
    pub expires: u64,
}

pub struct AuthService {
    accounts: Arc<dyn AccountRepository>,
    sessions: Arc<dyn SessionRepository>,
    once: Arc<dyn OneTimeRepository>,
    passwords: Arc<Passwords>,
    keys: RwLock<KeyRing>,
    clock: Arc<dyn Clock>,
    mailer: Arc<dyn Mailer>,
    lifetimes: Lifetimes,
    policy: TokenPolicy,
    moderators: Vec<String>,
    public_url: String,
}

pub fn random_hex(bytes: usize) -> Result<String, AuthError> {
    let mut raw = vec![0u8; bytes];
    getrandom::fill(&mut raw).map_err(|_| AuthError::Random)?;
    Ok(token::hmac::hex(&raw))
}

impl AuthService {
    pub fn new(parts: AuthParts) -> AuthService {
        AuthService {
            accounts: parts.accounts,
            sessions: parts.sessions,
            once: parts.once,
            passwords: parts.passwords,
            keys: RwLock::new(parts.keys),
            clock: parts.clock,
            mailer: parts.mailer,
            lifetimes: parts.lifetimes,
            policy: TokenPolicy {
                leeway: parts.lifetimes.leeway,
                ..TokenPolicy::default()
            },
            moderators: parts.moderators,
            public_url: parts.public_url,
        }
    }

    pub fn lifetimes(&self) -> Lifetimes {
        self.lifetimes
    }

    pub fn now(&self) -> u64 {
        self.clock.now()
    }

    pub fn rotate_key(&self, kid: &str, key: Vec<u8>) {
        let mut keys = self.keys.write().unwrap_or_else(PoisonError::into_inner);
        keys.rotate(kid, key);
    }

    pub fn retire_key(&self, kid: &str) -> bool {
        let mut keys = self.keys.write().unwrap_or_else(PoisonError::into_inner);
        keys.retire(kid)
    }

    fn sign(&self, account: &Account, row: &SessionRow, now: u64) -> String {
        let claims = Claims {
            sub: account.user_id,
            name: account.username.clone(),
            role: account.role().as_str().to_string(),
            iss: self.policy.issuer.clone(),
            aud: self.policy.audience.clone(),
            iat: now,
            nbf: now,
            exp: row.access_exp,
            jti: row.access_jti.clone(),
        };
        let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
        token::sign(&keys, &claims)
    }

    fn opening(&self, now: u64) -> Result<(String, Opening), AuthError> {
        let refresh = random_hex(16)?;
        let opening = Opening {
            refresh_hash: token::hmac::sha256_hex(&refresh),
            access_jti: random_hex(16)?,
            access_exp: now + self.lifetimes.access,
            expires: now + self.lifetimes.refresh,
        };
        Ok((refresh, opening))
    }

    fn issued(&self, account: Account, row: &SessionRow, refresh: String, now: u64) -> Issued {
        Issued {
            access: self.sign(&account, row, now),
            refresh,
            expires_in: self.lifetimes.access,
            account,
        }
    }

    pub async fn register(
        &self,
        profiles: &dyn ProfileDirectory,
        registration: &Registration,
    ) -> Result<Account, AuthError> {
        let hashed = self.passwords.hash(&registration.password).await?;
        let name = &registration.username;
        let profile = profiles
            .create(name.as_ref().to_string())
            .await
            .map_err(AuthError::Profile)?;
        let moderator = self.moderators.iter().any(|known| known == name.as_ref());
        let role = if moderator {
            Role::Moderator
        } else {
            Role::User
        };
        let account = Account {
            email: registration.email.as_ref().map(ToString::to_string),
            password: Some(hashed),
            role: role.as_str().to_string(),
            ..profile
        };
        match self.accounts.create_if_absent(account).await {
            Ok(account) => Ok(account),
            Err(RepoError::Conflict) => Err(AuthError::Duplicate(name.as_ref().to_string())),
            Err(RepoError::Storage(error)) => Err(AuthError::Storage(error)),
        }
    }

    pub async fn login(
        &self,
        name: &Username,
        password: &Secret<String>,
    ) -> Result<Issued, AuthError> {
        let found = self.accounts.find(name.as_ref().to_string()).await;
        let stored = found
            .as_ref()
            .and_then(|account| account.password.as_deref());
        let verdict = self.passwords.verify(password, stored).await?;
        let (Some(mut account), Verdict::Right { rehashed }) = (found, verdict) else {
            return Err(AuthError::InvalidCredentials);
        };
        if let Some(rehashed) = rehashed {
            account.password = Some(rehashed);
            self.accounts.update(account.clone()).await?;
        }
        let now = self.clock.now();
        let (refresh, opening) = self.opening(now)?;
        let row = self.sessions.open(account.user_id, opening).await;
        Ok(self.issued(account, &row, refresh, now))
    }

    pub async fn refresh(&self, presented: &str) -> Result<Issued, AuthError> {
        let now = self.clock.now();
        let (refresh, opening) = self.opening(now)?;
        let old = token::hmac::sha256_hex(presented);
        let row = self
            .sessions
            .rotate(old, now, opening)
            .await
            .map_err(AuthError::Refresh)?;
        let Some(account) = self.accounts.get(row.user_id).await else {
            self.sessions.revoke_family(row.family).await;
            return Err(AuthError::TokenInvalid);
        };
        Ok(self.issued(account, &row, refresh, now))
    }

    pub async fn validate(&self, access: &str) -> Result<Principal, AuthError> {
        let now = self.clock.now();
        let verified = {
            let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
            token::verify(&keys, access, &self.policy, now)
        };
        let claims = verified.map_err(|error| match error {
            VerifyError::Expired => AuthError::TokenExpired,
            _ => AuthError::TokenInvalid,
        })?;
        if self.sessions.is_denied(claims.jti.clone()).await {
            return Err(AuthError::TokenRevoked);
        }
        Ok(Principal {
            user_id: claims.sub,
            username: claims.name,
            role: Role::parse(&claims.role),
            jti: claims.jti,
            expires: claims.exp,
        })
    }

    pub async fn account(&self, user_id: u64) -> Option<Account> {
        self.accounts.get(user_id).await
    }

    pub async fn purge(&self) -> usize {
        self.sessions.purge(self.clock.now()).await
    }
}
