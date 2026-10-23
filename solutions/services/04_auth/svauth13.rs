
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

pub fn sha256_hex(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    pub fn new(start: u64) -> ManualClock {
        ManualClock(std::sync::atomic::AtomicU64::new(start))
    }

    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub const REFRESH_TTL: u64 = 30 * 24 * 3600;
pub const ACCESS_TTL: u64 = 900;

#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub access: String,
    pub refresh: String,
    pub access_expires: u64,
}

#[derive(Debug, PartialEq)]
pub enum RefreshError {
    Invalid,
    Expired,
}

struct Row {
    user: String,
    used: bool,
    expires: u64,
}

pub struct Sessions {
    clock: Arc<dyn Clock>,
    rows: Mutex<HashMap<String, Row>>,
}

impl Sessions {
    pub fn new(clock: Arc<dyn Clock>) -> Sessions {
        Sessions { clock, rows: Mutex::new(HashMap::new()) }
    }

    pub fn login(&self, user: &str) -> Pair {
        let mut rows = self.rows.lock().unwrap();
        self.issue(&mut rows, user)
    }

    pub fn refresh(&self, token: &str) -> Result<Pair, RefreshError> {
        let mut rows = self.rows.lock().unwrap();
        let now = self.clock.now();
        let row = rows.get_mut(&sha256_hex(token)).ok_or(RefreshError::Invalid)?;
        if row.used {
            return Err(RefreshError::Invalid);
        }
        if row.expires <= now {
            return Err(RefreshError::Expired);
        }
        row.used = true;
        let user = row.user.clone();
        Ok(self.issue(&mut rows, &user))
    }

    fn issue(&self, rows: &mut HashMap<String, Row>, user: &str) -> Pair {
        let now = self.clock.now();
        let refresh = random_token();
        rows.insert(sha256_hex(&refresh), Row { user: user.to_string(), used: false, expires: now + REFRESH_TTL });
        Pair { access: random_token(), refresh, access_expires: now + ACCESS_TTL }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<ManualClock>, Sessions) {
        let clock = Arc::new(ManualClock::new(1_000));
        (clock.clone(), Sessions::new(clock))
    }

    #[test]
    fn login_issues_a_pair() {
        let (_, sessions) = setup();
        let pair = sessions.login("alice");
        assert_ne!(pair.access, pair.refresh, "access и refresh это разные секреты");
        assert_eq!(pair.access_expires, 1_000 + ACCESS_TTL, "access живёт недолго");
    }

    #[test]
    fn refresh_gives_a_new_pair() {
        let (_, sessions) = setup();
        let first = sessions.login("alice");
        let second = sessions.refresh(&first.refresh).expect("первый обмен проходит");
        assert_ne!(second.refresh, first.refresh, "refresh меняется при каждом обмене");
        assert_ne!(second.access, first.access, "access тоже новый");
    }

    #[test]
    fn used_refresh_is_dead() {
        let (_, sessions) = setup();
        let first = sessions.login("alice");
        sessions.refresh(&first.refresh).unwrap();
        assert!(sessions.refresh(&first.refresh).is_err(), "старый refresh после обмена принимать нельзя: иначе вор, которому токен достался раньше, так и живёт");
    }

    #[test]
    fn chain_of_rotations_keeps_working() {
        let (_, sessions) = setup();
        let mut pair = sessions.login("alice");
        for round in 0..6 {
            pair = sessions.refresh(&pair.refresh).unwrap_or_else(|e| panic!("обмен {round}: {e:?}"));
        }
        assert!(sessions.refresh(&pair.refresh).is_ok(), "цепочка обменов не обрывается");
    }

    #[test]
    fn unknown_token_is_invalid() {
        let (_, sessions) = setup();
        assert_eq!(sessions.refresh("nope"), Err(RefreshError::Invalid), "такого токена не выдавали");
    }

    #[test]
    fn refresh_expires_and_renewal_starts_a_new_window() {
        let (clock, sessions) = setup();
        let first = sessions.login("alice");
        clock.advance(REFRESH_TTL - 10);
        let second = sessions.refresh(&first.refresh).expect("за 10 секунд до конца ещё можно");
        clock.advance(REFRESH_TTL - 10);
        assert!(sessions.refresh(&second.refresh).is_ok(), "после обмена срок отсчитывается заново");
        let third = sessions.login("bob");
        clock.advance(REFRESH_TTL);
        assert_eq!(sessions.refresh(&third.refresh), Err(RefreshError::Expired), "через 30 суток токен просрочен");
    }

    #[test]
    fn users_are_independent() {
        let (_, sessions) = setup();
        let a = sessions.login("alice");
        let b = sessions.login("bob");
        sessions.refresh(&a.refresh).unwrap();
        assert!(sessions.refresh(&b.refresh).is_ok(), "обмен alice не трогает bob");
    }
}
