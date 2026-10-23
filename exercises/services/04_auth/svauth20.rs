// Письмо «подтвердите почту» и письмо «сбросить пароль» содержат одноразовые ссылки. Ссылка живёт
// ограниченное время, работает один раз, и ссылку для сброса пароля нельзя использовать как ссылку
// подтверждения (и наоборот). В базе хранится только хеш токена, потому что утечка таблицы не должна
// давать доступ к сбросам пароля. Цель: `issue` и `consume`. Подтверждение почты живёт 24 часа, сброс
// пароля час. Новая ссылка того же назначения для того же пользователя отменяет прежнюю. Два
// одновременных `consume` одним токеном дают один успех.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    ConfirmEmail,
    ResetPassword,
}

impl Purpose {
    pub fn ttl(self) -> u64 {
        match self {
            Purpose::ConfirmEmail => 24 * 3600,
            Purpose::ResetPassword => 3600,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ConsumeError {
    Invalid,
    Expired,
}

struct Entry {
    user: String,
    purpose: Purpose,
    expires: u64,
}

pub struct OneTime {
    clock: Arc<dyn Clock>,
    entries: Mutex<HashMap<String, Entry>>,
}

impl OneTime {
    pub fn new(clock: Arc<dyn Clock>) -> OneTime {
        OneTime { clock, entries: Mutex::new(HashMap::new()) }
    }

    pub fn dump(&self) -> Vec<String> {
        self.entries.lock().unwrap().keys().cloned().collect()
    }

    pub fn issue(&self, user: &str, purpose: Purpose) -> String {
        todo!()
    }

    pub fn consume(&self, token: &str, purpose: Purpose) -> Result<String, ConsumeError> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<ManualClock>, Arc<OneTime>) {
        let clock = Arc::new(ManualClock::new(1_000));
        (clock.clone(), Arc::new(OneTime::new(clock)))
    }

    #[test]
    fn token_works_once() {
        let (_, tokens) = setup();
        let token = tokens.issue("alice", Purpose::ConfirmEmail);
        assert_eq!(tokens.consume(&token, Purpose::ConfirmEmail), Ok("alice".to_string()), "первый раз");
        assert_eq!(tokens.consume(&token, Purpose::ConfirmEmail), Err(ConsumeError::Invalid), "второй раз тот же токен уже недействителен");
    }

    #[test]
    fn purposes_are_not_interchangeable_and_a_wrong_try_does_not_burn_the_token() {
        let (_, tokens) = setup();
        let token = tokens.issue("alice", Purpose::ResetPassword);
        assert_eq!(tokens.consume(&token, Purpose::ConfirmEmail), Err(ConsumeError::Invalid), "ссылка сброса не подтверждает почту");
        assert_eq!(tokens.consume(&token, Purpose::ResetPassword), Ok("alice".to_string()), "по назначению всё ещё работает");
    }

    #[test]
    fn lifetimes_depend_on_the_purpose() {
        let (clock, tokens) = setup();
        let reset = tokens.issue("alice", Purpose::ResetPassword);
        let confirm = tokens.issue("alice", Purpose::ConfirmEmail);
        clock.advance(3_599);
        assert!(tokens.consume(&reset, Purpose::ResetPassword).is_ok(), "сброс живёт час");
        let reset2 = tokens.issue("alice", Purpose::ResetPassword);
        clock.advance(3_600);
        assert_eq!(tokens.consume(&reset2, Purpose::ResetPassword), Err(ConsumeError::Expired), "через час уже поздно");
        assert!(tokens.consume(&confirm, Purpose::ConfirmEmail).is_ok(), "подтверждение почты ещё живо (сутки)");
    }

    #[test]
    fn newer_link_cancels_the_older_one() {
        let (_, tokens) = setup();
        let first = tokens.issue("alice", Purpose::ResetPassword);
        let second = tokens.issue("alice", Purpose::ResetPassword);
        assert_eq!(tokens.consume(&first, Purpose::ResetPassword), Err(ConsumeError::Invalid), "старая ссылка отменена новой");
        assert!(tokens.consume(&second, Purpose::ResetPassword).is_ok(), "новая работает");
        let bob = tokens.issue("bob", Purpose::ResetPassword);
        tokens.issue("alice", Purpose::ResetPassword);
        assert!(tokens.consume(&bob, Purpose::ResetPassword).is_ok(), "ссылка другого пользователя не отменяется");
    }

    #[test]
    fn only_hashes_are_stored() {
        let (_, tokens) = setup();
        let token = tokens.issue("alice", Purpose::ResetPassword);
        let dump = tokens.dump();
        assert!(!dump.iter().any(|row| row.contains(&token)), "в таблице не должно быть самого токена");
        for row in dump {
            assert_eq!(tokens.consume(&row, Purpose::ResetPassword), Err(ConsumeError::Invalid), "строка из дампа не открывает сброс");
        }
    }

    #[test]
    fn simultaneous_use_has_one_winner() {
        let (_, tokens) = setup();
        let token = tokens.issue("alice", Purpose::ResetPassword);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (tokens, barrier, token) = (tokens.clone(), barrier.clone(), token.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    tokens.consume(&token, Purpose::ResetPassword).is_ok()
                })
            })
            .collect();
        let winners = handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count();
        assert_eq!(winners, 1, "одним токеном сменили пароль {winners} раз");
    }

    #[test]
    fn unknown_token_is_invalid() {
        let (_, tokens) = setup();
        assert_eq!(tokens.consume("nope", Purpose::ConfirmEmail), Err(ConsumeError::Invalid), "такого не выдавали");
    }
}
