// Мобильный клиент подключился к шлюзу по WebSocket. Чтобы события доходили до него, шлюз подписывается в NATS на тему
// пользователя `<префикс>.user.<ник>` и пересылает всё, что туда придёт, в канал соединения. Шлюзов может быть несколько:
// каждый слушает только своих пользователей. Цель: connect проверяет ник (узкий алфавит, как в прошлых главах: пустой,
// со звёздочкой, точкой или пробелом нельзя), подписывается и возвращает управление, когда брокер уже знает про подписку;
// disconnect отписывает шлюз так, что в брокере подписчика больше нет. Пока у пользователя одно соединение.

use async_nats::Client;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

fn main() {}

pub struct Connection {
    pub id: u64,
    pub user: String,
    pub inbox: mpsc::UnboundedReceiver<String>,
}

struct Live {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub struct Gateway {
    client: Client,
    prefix: String,
    next: AtomicU64,
    live: Mutex<HashMap<u64, Live>>,
}

fn valid_user(user: &str) -> bool {
    !user.is_empty() && user.len() <= 32 && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

async fn settle(client: &Client) -> Result<(), String> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(|e| e.to_string())?;
    client.publish(inbox, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await.map(|_| ()).ok_or_else(|| "соединение закрылось".to_string())
}

impl Gateway {
    pub fn new(client: Client, prefix: &str) -> Gateway {
        Gateway { client, prefix: prefix.to_string(), next: AtomicU64::new(0), live: Mutex::new(HashMap::new()) }
    }

    pub fn user_subject(&self, user: &str) -> String {
        format!("{}.user.{user}", self.prefix)
    }

    pub async fn connect(&self, user: &str) -> Result<Connection, String> {
        let _ = (user, valid_user(user), &self.next, &self.live);
        todo!()
    }

    pub async fn disconnect(&self, connection: &Connection) {
        let _ = (connection, settle(&self.client));
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::client::RequestErrorKind;
    use course_support::nats::{Proxy, Scope, connect as nats, connect_to};
    use std::time::Duration;
    use tokio::time::timeout;

    async fn next(connection: &mut Connection) -> Option<String> {
        timeout(Duration::from_secs(2), connection.inbox.recv()).await.ok().flatten()
    }

    async fn silent(connection: &mut Connection) -> bool {
        timeout(Duration::from_millis(300), connection.inbox.recv()).await.is_err()
    }

    async fn nobody_listens(client: &Client, subject: &str) -> bool {
        match timeout(Duration::from_secs(1), client.request(subject.to_string(), "".into())).await {
            Ok(Err(error)) => error.kind() == RequestErrorKind::NoResponders,
            _ => false,
        }
    }

    #[tokio::test]
    async fn a_message_reaches_the_connected_user_and_nobody_else() {
        let scope = Scope::new();
        let gateway = Gateway::new(nats().await, scope.prefix());
        let mut alice = gateway.connect("alice").await.expect("alice");
        let mut bob = gateway.connect("bob").await.expect("bob");
        let publisher = nats().await;
        publisher.publish(gateway.user_subject("alice"), "привет, alice".into()).await.unwrap();
        publisher.flush().await.unwrap();
        assert_eq!(next(&mut alice).await.as_deref(), Some("привет, alice"), "alice должна получить событие");
        assert!(silent(&mut bob).await, "bob чужого не получает");
    }

    #[tokio::test]
    async fn every_gateway_serves_only_its_own_users() {
        let scope = Scope::new();
        let (a, b) = (Gateway::new(nats().await, scope.prefix()), Gateway::new(nats().await, scope.prefix()));
        let mut alice_on_a = a.connect("alice").await.unwrap();
        let mut bob_on_b = b.connect("bob").await.unwrap();
        let mut alice_on_b = b.connect("alice").await.unwrap();
        let publisher = nats().await;
        publisher.publish(a.user_subject("alice"), "для alice".into()).await.unwrap();
        publisher.flush().await.unwrap();
        assert_eq!(next(&mut alice_on_a).await.as_deref(), Some("для alice"), "alice на первом шлюзе");
        assert_eq!(next(&mut alice_on_b).await.as_deref(), Some("для alice"), "alice на втором шлюзе: событие приходит на оба шлюза");
        assert!(silent(&mut bob_on_b).await, "bob события alice не видит");
    }

    #[tokio::test]
    async fn hostile_names_are_refused_before_anything_is_subscribed() {
        let scope = Scope::new();
        let client = nats().await;
        let gateway = Gateway::new(client.clone(), scope.prefix());
        for name in ["", "*", ">", "a.b", "a b", "x\r\ny", &"n".repeat(33)] {
            assert!(gateway.connect(name).await.is_err(), "ник {name:?} должен быть отвергнут");
        }
        let mut alice = gateway.connect("alice").await.unwrap();
        client.publish(gateway.user_subject("bob"), "чужое".into()).await.unwrap();
        client.flush().await.unwrap();
        assert!(silent(&mut alice).await, "отвергнутые ники не должны были оставить подписку-пылесос");
        assert!(nobody_listens(&client, &gateway.user_subject("zed")).await, "на тему постороннего никто не подписан");
    }

    #[tokio::test]
    async fn after_disconnect_the_broker_has_no_subscriber_left() {
        let scope = Scope::new();
        let client = nats().await;
        let gateway = Gateway::new(client.clone(), scope.prefix());
        let alice = gateway.connect("alice").await.unwrap();
        assert!(!nobody_listens(&client, &gateway.user_subject("alice")).await, "пока alice подключена, подписчик есть");
        gateway.disconnect(&alice).await;
        assert!(nobody_listens(&client, &gateway.user_subject("alice")).await, "после disconnect подписки быть не должно: иначе шлюз вечно копит подписки отключившихся");
    }

    #[tokio::test]
    async fn connect_returns_when_the_broker_already_knows_the_subscription() {
        let proxy = Proxy::start().await;
        proxy.set_latency(Duration::from_millis(150));
        let scope = Scope::new();
        let slow = connect_to(&proxy.url(), async_nats::ConnectOptions::new()).await;
        let gateway = Gateway::new(slow, scope.prefix());
        let mut alice = gateway.connect("alice").await.unwrap();
        let publisher = nats().await;
        publisher.publish(gateway.user_subject("alice"), "сразу после connect".into()).await.unwrap();
        publisher.flush().await.unwrap();
        assert_eq!(next(&mut alice).await.as_deref(), Some("сразу после connect"), "connect вернулся, а брокер ещё не знает про подписку: нужен круг до брокера");
    }
}
