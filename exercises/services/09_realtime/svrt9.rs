// Алиса нажала «выйти на всех устройствах» в приложении, которое подключено к шлюзу A. Сессию отозвали, и соединения
// на шлюзе A закрылись. А её планшет сидит на шлюзе B и продолжает получать сообщения: B ничего не узнал. Цель:
// отзыв сессии доходит до всех шлюзов через NATS (тема `<префикс>.session.revoked`, в теле идентификатор сессии) и
// закрывает на каждом соединения этой сессии. Отозванная сессия не может открыть новое соединение (Refused::Revoked) ни на
// одном шлюзе. Шлюз помнит не больше 1000 последних отозванных. Метод start возвращается, когда шлюз уже слушает отзывы,
// а logout закрывает локальные соединения сразу, не дожидаясь ответа сети.

use async_nats::Client;
use futures::StreamExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

fn main() {}

const REMEMBERED: usize = 1000;

pub struct Conn {
    pub id: u64,
    pub user: String,
    pub session: String,
    closed: Arc<AtomicBool>,
}

impl Conn {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[derive(Debug, PartialEq)]
pub enum Refused {
    Revoked,
    BadSession,
}

#[derive(Default)]
struct State {
    open: HashMap<u64, (String, Arc<AtomicBool>)>,
    order: VecDeque<String>,
    revoked: HashSet<String>,
}

impl State {
    fn revoke(&mut self, session: &str) {
        for (open_session, closed) in self.open.values() {
            if open_session == session {
                closed.store(true, Ordering::SeqCst);
            }
        }
        if self.revoked.insert(session.to_string()) {
            self.order.push_back(session.to_string());
            if self.order.len() > REMEMBERED
                && let Some(oldest) = self.order.pop_front()
            {
                self.revoked.remove(&oldest);
            }
        }
    }
}

pub struct Gateway {
    client: Client,
    subject: String,
    state: Arc<Mutex<State>>,
    next: AtomicU64,
    listener: Mutex<Option<JoinHandle<()>>>,
}

async fn settle(client: &Client) -> Result<(), String> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(|e| e.to_string())?;
    client.publish(inbox, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await.map(|_| ()).ok_or_else(|| "соединение закрылось".to_string())
}

impl Gateway {
    pub async fn start(client: Client, prefix: &str) -> Result<Gateway, String> {
        let gateway = Gateway {
            client,
            subject: format!("{prefix}.session.revoked"),
            state: Arc::new(Mutex::new(State::default())),
            next: AtomicU64::new(0),
            listener: Mutex::new(None),
        };
        let _ = (&gateway.client, settle(&gateway.client));
        Ok(gateway)
    }

    pub fn open(&self, user: &str, session: &str) -> Result<Conn, Refused> {
        if session.is_empty() || session.len() > 64 {
            return Err(Refused::BadSession);
        }
        let mut state = self.state.lock().unwrap();
        if state.revoked.contains(session) {
            return Err(Refused::Revoked);
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let closed = Arc::new(AtomicBool::new(false));
        state.open.insert(id, (session.to_string(), closed.clone()));
        Ok(Conn { id, user: user.to_string(), session: session.to_string(), closed })
    }

    pub async fn logout(&self, session: &str) -> Result<(), String> {
        self.state.lock().unwrap().revoke(session);
        Ok(())
    }

    pub fn remembered(&self) -> usize {
        self.state.lock().unwrap().revoked.len()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        if let Some(task) = self.listener.lock().unwrap().take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Proxy, Scope, connect, connect_to, wait_until};
    use std::time::Duration;

    #[tokio::test]
    async fn a_logout_on_one_gateway_closes_the_session_on_all_of_them() {
        let scope = Scope::new();
        let a = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        let b = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        let phone_on_a = a.open("alice", "s1").unwrap();
        let laptop_on_b = b.open("alice", "s1").unwrap();
        let tablet_on_a = a.open("alice", "s2").unwrap();
        let bob_on_b = b.open("bob", "s3").unwrap();
        a.logout("s1").await.expect("отзыв отправлен");
        assert!(phone_on_a.is_closed(), "на шлюзе, где нажали выход, соединение закрыто сразу, как только logout вернулся");
        wait_until("соединение на шлюзе B закрылось", Duration::from_secs(3), || async { laptop_on_b.is_closed() }).await;
        assert!(!tablet_on_a.is_closed(), "другая сессия alice остаётся");
        assert!(!bob_on_b.is_closed(), "чужая сессия остаётся");
    }

    #[tokio::test]
    async fn a_revoked_session_cannot_come_back_on_any_gateway() {
        let scope = Scope::new();
        let a = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        let b = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        a.logout("s1").await.unwrap();
        assert_eq!(a.open("alice", "s1").err(), Some(Refused::Revoked), "на шлюзе A отозванная сессия отвергнута");
        wait_until("шлюз B узнал про отзыв", Duration::from_secs(3), || async { b.open("alice", "s1").is_err() }).await;
        assert_eq!(b.open("alice", "s1").err(), Some(Refused::Revoked), "и на шлюзе B");
        assert!(b.open("alice", "s2").is_ok(), "новая сессия того же пользователя допустима");
    }

    #[tokio::test]
    async fn a_gateway_on_a_slow_link_is_already_listening_when_start_returns() {
        let proxy = Proxy::start().await;
        proxy.set_latency(Duration::from_millis(150));
        let scope = Scope::new();
        let a = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        let slow = connect_to(&proxy.url(), async_nats::ConnectOptions::new()).await;
        let b = Gateway::start(slow, scope.prefix()).await.unwrap();
        let laptop = b.open("alice", "s1").unwrap();
        a.logout("s1").await.unwrap();
        wait_until("медленный шлюз получил отзыв", Duration::from_secs(3), || async { laptop.is_closed() }).await;
    }

    #[tokio::test]
    async fn the_memory_of_revoked_sessions_is_bounded() {
        let scope = Scope::new();
        let a = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        let b = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        for i in 0..1500 {
            a.logout(&format!("session-{i}")).await.unwrap();
        }
        wait_until("шлюз B получил все отзывы", Duration::from_secs(5), || async { b.open("zed", "session-1499").is_err() }).await;
        assert_eq!(a.remembered(), 1000, "помним последние 1000, а не все");
        assert_eq!(b.remembered(), 1000, "и на втором шлюзе тоже");
        assert!(a.open("zed", "session-0").is_ok(), "самый старый отзыв уже забыт: цена ограниченной памяти (токен сам к тому времени истёк)");
    }

    #[tokio::test]
    async fn an_empty_session_is_refused() {
        let scope = Scope::new();
        let a = Gateway::start(connect().await, scope.prefix()).await.unwrap();
        assert_eq!(a.open("alice", "").err(), Some(Refused::BadSession), "без идентификатора сессии соединение не открывается");
    }
}
