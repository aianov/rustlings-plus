// Ротация работает, но атакующий с украденным токеном и хозяин с тем же токеном нажали «обновить» в одну
// и ту же долю секунды, и оба получили новые пары: проверка «токен ещё не использован» и пометка «использован»
// разделены обращением к базе, а между ними успевает вклиниться второй запрос. Хуже того, даже
// последовательный повтор старого токена ничего не меняет в судьбе остальных: а он признак кражи. Цель:
// из одновременных обменов одним токеном успешен ровно один, а повторное предъявление (когда токен уже
// использован) гасит всё семейство токенов этого входа, включая свежий. Сначала смотрим, не использован ли
// токен, и только потом остальное: все проигравшие гонку получают `Reused`.

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub access: String,
    pub refresh: String,
}

#[derive(Debug, PartialEq)]
pub enum RefreshError {
    Invalid,
    Expired,
    Reused,
}

#[derive(Clone)]
struct Row {
    family: u64,
    used: bool,
    expires: u64,
}

#[derive(Default)]
struct State {
    rows: HashMap<String, Row>,
    revoked_families: Vec<u64>,
    next_family: u64,
}

pub struct Sessions {
    clock: Arc<dyn Clock>,
    latency: Duration,
    state: Mutex<State>,
}

impl Sessions {
    pub fn new(clock: Arc<dyn Clock>, latency: Duration) -> Sessions {
        Sessions { clock, latency, state: Mutex::new(State::default()) }
    }

    pub fn login(&self, _user: &str) -> Pair {
        let mut state = self.state.lock().unwrap();
        state.next_family += 1;
        let family = state.next_family;
        self.insert(&mut state, family)
    }

    fn insert(&self, state: &mut State, family: u64) -> Pair {
        let refresh = random_token();
        state.rows.insert(sha256_hex(&refresh), Row { family, used: false, expires: self.clock.now() + REFRESH_TTL });
        Pair { access: random_token(), refresh }
    }

    pub fn refresh(&self, token: &str) -> Result<Pair, RefreshError> {
        let key = sha256_hex(token);
        let row = {
            let state = self.state.lock().unwrap();
            state.rows.get(&key).cloned().ok_or(RefreshError::Invalid)?
        };
        if row.used {
            return Err(RefreshError::Invalid);
        }
        if row.expires <= self.clock.now() {
            return Err(RefreshError::Expired);
        }
        std::thread::sleep(self.latency);
        let mut state = self.state.lock().unwrap();
        state.rows.get_mut(&key).expect("строка на месте").used = true;
        Ok(self.insert(&mut state, row.family))
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    fn setup(latency_ms: u64) -> Arc<Sessions> {
        Arc::new(Sessions::new(Arc::new(ManualClock::new(1_000)), Duration::from_millis(latency_ms)))
    }

    #[test]
    fn plain_rotation_still_works() {
        let sessions = setup(0);
        let first = sessions.login("alice");
        let second = sessions.refresh(&first.refresh).expect("обычный обмен");
        assert!(sessions.refresh(&second.refresh).is_ok(), "цепочка продолжается");
        assert_eq!(sessions.refresh("nope"), Err(RefreshError::Invalid), "неизвестный токен");
    }

    #[test]
    fn reuse_kills_the_whole_family() {
        let sessions = setup(0);
        let first = sessions.login("alice");
        let second = sessions.refresh(&first.refresh).unwrap();
        assert_eq!(sessions.refresh(&first.refresh), Err(RefreshError::Reused), "повтор использованного токена это признак кражи");
        assert!(sessions.refresh(&second.refresh).is_err(), "свежий токен того же входа тоже гасится: неизвестно, кто из двоих настоящий, пусть войдут заново");
        assert!(sessions.refresh(&second.refresh).is_err(), "и остаётся погашенным");
    }

    #[test]
    fn other_logins_are_not_affected() {
        let sessions = setup(0);
        let phone = sessions.login("alice");
        let laptop = sessions.login("alice");
        let moved = sessions.refresh(&phone.refresh).unwrap();
        assert_eq!(sessions.refresh(&phone.refresh), Err(RefreshError::Reused), "кража на телефоне");
        assert!(sessions.refresh(&laptop.refresh).is_ok(), "ноутбук (другое семейство) продолжает работать");
        assert!(sessions.refresh(&moved.refresh).is_err(), "а семейство телефона погашено");
        assert!(sessions.login("alice").refresh.len() >= 64, "новый вход начинается с чистого листа");
    }

    #[test]
    fn simultaneous_refresh_with_one_token_has_exactly_one_winner() {
        let sessions = setup(30);
        let first = sessions.login("alice");
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (sessions, barrier, token) = (sessions.clone(), barrier.clone(), first.refresh.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    sessions.refresh(&token)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
        assert_eq!(winners.len(), 1, "из восьми одновременных обменов одним токеном должен пройти один, прошло {}", winners.len());
        assert!(results.iter().filter(|r| r.is_err()).all(|r| *r == Err(RefreshError::Reused)), "остальные получают отказ с признаком повтора: {results:?}");
        assert!(sessions.refresh(&winners[0].refresh).is_err(), "повтор был, значит, семейство погашено и у победителя");
    }
}
