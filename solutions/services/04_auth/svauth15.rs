
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

pub const ACCESS_TTL: u64 = 900;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub sid: u64,
    pub jti: String,
    pub user: String,
    pub exp: u64,
}

struct Session {
    token: Token,
    alive: bool,
}

#[derive(Default)]
struct State {
    sessions: HashMap<u64, Session>,
    denied: HashMap<String, u64>,
    next: u64,
}

pub struct Auth {
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl Auth {
    pub fn new(clock: Arc<dyn Clock>) -> Auth {
        Auth { clock, state: Mutex::new(State::default()) }
    }

    pub fn login(&self, user: &str) -> Token {
        let mut state = self.state.lock().unwrap();
        state.next += 1;
        let token = Token { sid: state.next, jti: format!("jti-{}", state.next), user: user.to_string(), exp: self.clock.now() + ACCESS_TTL };
        state.sessions.insert(token.sid, Session { token: token.clone(), alive: true });
        token
    }

    pub fn check(&self, token: &Token) -> bool {
        let state = self.state.lock().unwrap();
        token.exp > self.clock.now() && !state.denied.contains_key(&token.jti)
    }

    pub fn session_alive(&self, sid: u64) -> bool {
        self.state.lock().unwrap().sessions.get(&sid).is_some_and(|s| s.alive)
    }

    pub fn logout(&self, token: &Token) {
        let mut state = self.state.lock().unwrap();
        Self::revoke(&mut state, token.sid);
    }

    pub fn logout_all(&self, user: &str) {
        let mut state = self.state.lock().unwrap();
        let sids: Vec<u64> = state.sessions.values().filter(|s| s.token.user == user).map(|s| s.token.sid).collect();
        for sid in sids {
            Self::revoke(&mut state, sid);
        }
    }

    pub fn change_password(&self, user: &str, keep: &Token) {
        let mut state = self.state.lock().unwrap();
        let sids: Vec<u64> = state.sessions.values().filter(|s| s.token.user == user && s.token.sid != keep.sid).map(|s| s.token.sid).collect();
        for sid in sids {
            Self::revoke(&mut state, sid);
        }
    }

    pub fn denylist_len(&self) -> usize {
        self.state.lock().unwrap().denied.len()
    }

    pub fn purge(&self) -> usize {
        let now = self.clock.now();
        let mut state = self.state.lock().unwrap();
        let before = state.denied.len();
        state.denied.retain(|_, exp| *exp > now);
        before - state.denied.len()
    }

    fn revoke(state: &mut State, sid: u64) {
        if let Some(session) = state.sessions.get_mut(&sid) {
            session.alive = false;
            let (jti, exp) = (session.token.jti.clone(), session.token.exp);
            state.denied.insert(jti, exp);
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<ManualClock>, Auth) {
        let clock = Arc::new(ManualClock::new(10_000));
        (clock.clone(), Auth::new(clock))
    }

    #[test]
    fn fresh_token_is_valid_until_it_expires() {
        let (clock, auth) = setup();
        let token = auth.login("alice");
        assert!(auth.check(&token) && auth.session_alive(token.sid), "свежий токен");
        clock.advance(ACCESS_TTL);
        assert!(!auth.check(&token), "по сроку токен недействителен и без denylist");
    }

    #[test]
    fn logout_revokes_this_session_only() {
        let (_, auth) = setup();
        let phone = auth.login("alice");
        let laptop = auth.login("alice");
        auth.logout(&phone);
        assert!(!auth.check(&phone) && !auth.session_alive(phone.sid), "телефон вышел: access в denylist, refresh погашен");
        assert!(auth.check(&laptop) && auth.session_alive(laptop.sid), "ноутбук работает");
    }

    #[test]
    fn logout_all_revokes_every_session_of_the_user_and_nobody_else() {
        let (_, auth) = setup();
        let a1 = auth.login("alice");
        let a2 = auth.login("alice");
        let bob = auth.login("bob");
        auth.logout_all("alice");
        assert!(!auth.check(&a1) && !auth.check(&a2), "все access alice отозваны, хотя подпись и срок у них в порядке");
        assert!(!auth.session_alive(a1.sid) && !auth.session_alive(a2.sid), "все refresh alice погашены");
        assert!(auth.check(&bob) && auth.session_alive(bob.sid), "bob не затронут");
    }

    #[test]
    fn change_password_keeps_the_current_session() {
        let (_, auth) = setup();
        let here = auth.login("alice");
        let elsewhere = auth.login("alice");
        auth.change_password("alice", &here);
        assert!(auth.check(&here) && auth.session_alive(here.sid), "текущее устройство остаётся в системе");
        assert!(!auth.check(&elsewhere) && !auth.session_alive(elsewhere.sid), "остальные выкинуты: пароль могли узнать посторонние");
    }

    #[test]
    fn denylist_entries_live_exactly_as_long_as_the_token() {
        let (clock, auth) = setup();
        let token = auth.login("alice");
        auth.logout(&token);
        assert_eq!(auth.denylist_len(), 1, "отзыв записан");
        assert_eq!(auth.purge(), 0, "пока токен мог бы быть действителен, запись нужна");
        clock.advance(ACCESS_TTL - 1);
        assert_eq!(auth.purge(), 0, "за секунду до срока запись ещё нужна");
        clock.advance(1);
        assert_eq!(auth.purge(), 1, "срок вышел: охранять нечего, запись удаляется");
        assert_eq!(auth.denylist_len(), 0, "список не растёт вечно");
    }

    #[test]
    fn repeated_logout_is_harmless() {
        let (_, auth) = setup();
        let token = auth.login("alice");
        auth.logout(&token);
        auth.logout(&token);
        auth.logout_all("alice");
        assert_eq!(auth.denylist_len(), 1, "повторный выход не плодит записей");
    }

    #[test]
    fn purge_removes_only_expired_entries() {
        let (clock, auth) = setup();
        let old = auth.login("alice");
        clock.advance(600);
        let young = auth.login("alice");
        auth.logout_all("alice");
        clock.advance(300);
        assert_eq!(auth.purge(), 1, "старый токен истёк, молодой ещё нет");
        assert!(!auth.check(&young), "молодой всё ещё отозван");
        assert!(!auth.check(&old), "старый недействителен по сроку");
        assert_eq!(auth.denylist_len(), 1, "остаётся запись молодого");
    }
}
