// Утечка таблицы сессий: в ней лежат сами refresh-токены, и теперь любой, кто прочитал строку, может
// войти как этот пользователь, пароль ему не нужен. Токен в базе должен лежать так же, как пароль: в виде
// хеша. Но токен случайный и длинный, поэтому тут подойдёт быстрый sha256 без соли. Цель: хранилище
// держит только хеши, выдача и поиск по исходному токену работают, а любая строка из дампа
// в роли токена не открывает ничего.

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

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

struct Session {
    user: String,
    expires: u64,
}

#[derive(Default)]
pub struct SessionStore {
    rows: Mutex<HashMap<String, Session>>,
}

impl SessionStore {
    pub fn new() -> SessionStore {
        SessionStore::default()
    }

    pub fn create(&self, user: &str, expires: u64) -> String {
        let token = random_token();
        self.rows.lock().unwrap().insert(token.clone(), Session { user: user.to_string(), expires });
        token
    }

    pub fn lookup(&self, token: &str, now: u64) -> Option<String> {
        let rows = self.rows.lock().unwrap();
        rows.get(token).filter(|s| s.expires > now).map(|s| s.user.clone())
    }

    pub fn dump(&self) -> Vec<String> {
        self.rows.lock().unwrap().keys().cloned().collect()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_opens_its_own_session() {
        let store = SessionStore::new();
        let token = store.create("alice", 1_000);
        assert_eq!(store.lookup(&token, 10), Some("alice".to_string()), "токен находит пользователя");
        assert_eq!(store.lookup("unknown", 10), None, "чужой токен");
        assert_eq!(store.lookup(&token, 1_000), None, "срок вышел");
    }

    #[test]
    fn tokens_are_long_and_unique() {
        let store = SessionStore::new();
        let a = store.create("a", 1_000);
        let b = store.create("a", 1_000);
        assert_ne!(a, b, "каждый вход получает свой токен");
        assert!(a.len() >= 64, "токен должен нести не меньше 256 бит случайности: {a}");
    }

    #[test]
    fn the_dump_contains_no_usable_secrets() {
        let store = SessionStore::new();
        let token = store.create("alice", 1_000);
        let dump = store.dump();
        assert_eq!(dump.len(), 1, "одна сессия, одна строка");
        assert!(!dump.contains(&token), "в таблице лежит сам токен: утечка таблицы равна входу за любого пользователя");
        for leaked in &dump {
            assert_eq!(store.lookup(leaked, 10), None, "строка из дампа не должна работать как токен");
        }
        assert!(dump.iter().all(|row| !row.contains(&token)), "и токен не прячется внутри строки");
    }

    #[test]
    fn two_users_do_not_collide() {
        let store = SessionStore::new();
        let a = store.create("alice", 1_000);
        let b = store.create("bob", 1_000);
        assert_eq!(store.lookup(&a, 0), Some("alice".to_string()), "alice");
        assert_eq!(store.lookup(&b, 0), Some("bob".to_string()), "bob");
    }
}
