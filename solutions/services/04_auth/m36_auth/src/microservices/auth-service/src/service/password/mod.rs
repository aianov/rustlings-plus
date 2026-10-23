use std::fmt;
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use service_runtime::Secret;
use tokio::sync::Semaphore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Cost {
    pub const STRONG: Cost = Cost {
        memory_kib: 19_456,
        iterations: 2,
        parallelism: 1,
    };

    pub const QUICK: Cost = Cost {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    };

    fn params(self) -> Result<Params, PasswordError> {
        Params::new(self.memory_kib, self.iterations, self.parallelism, None)
            .map_err(|error| PasswordError::Params(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    Params(String),
    Hash(String),
    Pool,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordError::Params(why) => write!(f, "параметры argon2 не годятся: {why}"),
            PasswordError::Hash(why) => write!(f, "argon2 не посчитал хеш: {why}"),
            PasswordError::Pool => write!(f, "пул хеширования закрыт"),
        }
    }
}

impl std::error::Error for PasswordError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Wrong,
    Right { rehashed: Option<String> },
}

fn argon(cost: Cost) -> Result<Argon2<'static>, PasswordError> {
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        cost.params()?,
    ))
}

fn salt() -> Result<SaltString, PasswordError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| PasswordError::Hash(error.to_string()))?;
    SaltString::encode_b64(&bytes).map_err(|error| PasswordError::Hash(error.to_string()))
}

fn hash_now(cost: Cost, password: &str) -> Result<String, PasswordError> {
    let salt = salt()?;
    let hashed = argon(cost)?
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| PasswordError::Hash(error.to_string()))?;
    Ok(hashed.to_string())
}

fn same_cost(record: &PasswordHash<'_>, cost: Cost) -> bool {
    Params::try_from(record).is_ok_and(|params| {
        params.m_cost() == cost.memory_kib
            && params.t_cost() == cost.iterations
            && params.p_cost() == cost.parallelism
    })
}

fn check_now(cost: Cost, password: &str, stored: &str) -> Result<Verdict, PasswordError> {
    let Ok(record) = PasswordHash::new(stored) else {
        return Ok(Verdict::Wrong);
    };
    let verifier = argon(cost)?;
    if verifier
        .verify_password(password.as_bytes(), &record)
        .is_err()
    {
        return Ok(Verdict::Wrong);
    }
    let rehashed = if same_cost(&record, cost) {
        None
    } else {
        Some(hash_now(cost, password)?)
    };
    Ok(Verdict::Right { rehashed })
}

pub struct Passwords {
    cost: Cost,
    permits: Arc<Semaphore>,
    dummy: String,
}

impl Passwords {
    pub fn new(cost: Cost, parallel: usize) -> Result<Passwords, PasswordError> {
        let mut filler = [0u8; 24];
        getrandom::fill(&mut filler).map_err(|error| PasswordError::Hash(error.to_string()))?;
        let dummy = hash_now(cost, &super::token::hmac::hex(&filler))?;
        Ok(Passwords {
            cost,
            permits: Arc::new(Semaphore::new(parallel.max(1))),
            dummy,
        })
    }

    pub fn cost(&self) -> Cost {
        self.cost
    }

    async fn blocking<T, F>(&self, work: F) -> Result<T, PasswordError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, PasswordError> + Send + 'static,
    {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| PasswordError::Pool)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|_| PasswordError::Pool)?
    }

    pub async fn hash(&self, password: &Secret<String>) -> Result<String, PasswordError> {
        let (cost, password) = (self.cost, password.expose().clone());
        self.blocking(move || hash_now(cost, &password)).await
    }

    pub async fn verify(
        &self,
        password: &Secret<String>,
        stored: Option<&str>,
    ) -> Result<Verdict, PasswordError> {
        let known = stored.is_some();
        let record = stored.unwrap_or(&self.dummy).to_string();
        let (cost, password) = (self.cost, password.expose().clone());
        let verdict = self
            .blocking(move || check_now(cost, &password, &record))
            .await?;
        Ok(if known { verdict } else { Verdict::Wrong })
    }
}
