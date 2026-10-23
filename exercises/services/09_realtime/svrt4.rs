// Шлюз работает, пока у пользователя одно устройство. Как только Алиса открыла приложение и на телефоне, и на ноутбуке,
// каждое событие стало приходить на оба устройства по два раза, а потом по три, если открыть третье. Шлюз подписывается
// в NATS на каждое соединение, и каждая такая подписка пересылает событие всем устройствам пользователя. Цель (поле `live` придётся переделать под этот счёт): на пользователя
// приходится одна подписка на шлюз, пока у него есть хотя бы одно соединение: создаётся при первом, снимается при
// последнем. Каждое устройство получает каждое событие один раз, в том числе при одновременном подключении.

use async_nats::Client;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

fn main() {}

pub struct Connection {
    pub id: u64,
    pub user: String,
    pub inbox: mpsc::UnboundedReceiver<String>,
}

type Conns = Arc<StdMutex<HashMap<String, Vec<(u64, UnboundedSender<String>)>>>>;

struct Live {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub struct Gateway {
    client: Client,
    prefix: String,
    next: AtomicU64,
    conns: Conns,
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
        Gateway { client, prefix: prefix.to_string(), next: AtomicU64::new(0), conns: Conns::default(), live: Mutex::new(HashMap::new()) }
    }

    pub fn user_subject(&self, user: &str) -> String {
        format!("{}.user.{user}", self.prefix)
    }

    async fn start_forwarder(&self, user: &str) -> Result<(oneshot::Sender<()>, JoinHandle<()>), String> {
        let mut subscription = self.client.subscribe(self.user_subject(user)).await.map_err(|e| e.to_string())?;
        settle(&self.client).await?;
        let (stop, mut stopped) = oneshot::channel();
        let (conns, owner) = (self.conns.clone(), user.to_string());
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = subscription.next() => {
                        let Some(message) = message else { return };
                        let text = String::from_utf8_lossy(&message.payload).into_owned();
                        if let Some(devices) = conns.lock().unwrap().get(&owner) {
                            for (_, tx) in devices {
                                tx.send(text.clone()).ok();
                            }
                        }
                    }
                    _ = &mut stopped => break,
                }
            }
            subscription.unsubscribe().await.ok();
        });
        Ok((stop, task))
    }

    pub async fn connect(&self, user: &str) -> Result<Connection, String> {
        if !valid_user(user) {
            return Err(format!("ник {user:?} не годится для темы"));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, inbox) = unbounded_channel();
        self.conns.lock().unwrap().entry(user.to_string()).or_default().push((id, tx));
        match self.start_forwarder(user).await {
            Ok((stop, task)) => {
                self.live.lock().await.insert(id, Live { stop, task });
            }
            Err(error) => {
                self.forget(user, id);
                return Err(error);
            }
        }
        Ok(Connection { id, user: user.to_string(), inbox })
    }

    fn forget(&self, user: &str, id: u64) {
        let mut conns = self.conns.lock().unwrap();
        if let Some(devices) = conns.get_mut(user) {
            devices.retain(|(conn, _)| *conn != id);
            if devices.is_empty() {
                conns.remove(user);
            }
        }
    }

    pub async fn disconnect(&self, connection: &Connection) {
        self.forget(&connection.user, connection.id);
        let Some(live) = self.live.lock().await.remove(&connection.id) else {
            return;
        };
        live.stop.send(()).ok();
        live.task.await.ok();
        settle(&self.client).await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::client::RequestErrorKind;
    use course_support::nats::{Scope, connect as nats};
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

    async fn publish(client: &Client, gateway: &Gateway, user: &str, text: &str) {
        client.publish(gateway.user_subject(user), text.to_string().into()).await.unwrap();
        client.flush().await.unwrap();
    }

    #[tokio::test]
    async fn two_devices_get_each_event_exactly_once() {
        let scope = Scope::new();
        let gateway = Gateway::new(nats().await, scope.prefix());
        let mut phone = gateway.connect("alice").await.unwrap();
        let mut laptop = gateway.connect("alice").await.unwrap();
        let publisher = nats().await;
        for i in 0..5 {
            publish(&publisher, &gateway, "alice", &format!("m{i}")).await;
        }
        for (name, device) in [("телефон", &mut phone), ("ноутбук", &mut laptop)] {
            for i in 0..5 {
                assert_eq!(next(device).await, Some(format!("m{i}")), "{name}: событие {i} по порядку");
            }
            assert!(silent(device).await, "{name}: лишних копий быть не должно (по подписке на каждое соединение их было бы несколько)");
        }
    }

    #[tokio::test]
    async fn closing_one_device_does_not_cut_the_other() {
        let scope = Scope::new();
        let gateway = Gateway::new(nats().await, scope.prefix());
        let phone = gateway.connect("alice").await.unwrap();
        let mut laptop = gateway.connect("alice").await.unwrap();
        gateway.disconnect(&phone).await;
        let publisher = nats().await;
        publish(&publisher, &gateway, "alice", "после отключения телефона").await;
        assert_eq!(next(&mut laptop).await.as_deref(), Some("после отключения телефона"), "ноутбук остаётся в сети и получает событие");
        assert!(silent(&mut laptop).await, "и ровно одну копию");
    }

    #[tokio::test]
    async fn the_last_disconnect_removes_the_subscription_and_a_new_connect_restores_it() {
        let scope = Scope::new();
        let client = nats().await;
        let gateway = Gateway::new(client.clone(), scope.prefix());
        let a = gateway.connect("alice").await.unwrap();
        let b = gateway.connect("alice").await.unwrap();
        gateway.disconnect(&a).await;
        assert!(!nobody_listens(&client, &gateway.user_subject("alice")).await, "одно устройство осталось, подписка нужна");
        gateway.disconnect(&b).await;
        assert!(nobody_listens(&client, &gateway.user_subject("alice")).await, "устройств не осталось: подписка должна исчезнуть");
        let mut again = gateway.connect("alice").await.unwrap();
        publish(&client, &gateway, "alice", "снова онлайн").await;
        assert_eq!(next(&mut again).await.as_deref(), Some("снова онлайн"), "после нового подключения события идут");
    }

    #[tokio::test]
    async fn simultaneous_first_connections_still_make_one_subscription() {
        let scope = Scope::new();
        let gateway = Arc::new(Gateway::new(nats().await, scope.prefix()));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let gateway = gateway.clone();
                tokio::spawn(async move { gateway.connect("alice").await.unwrap() })
            })
            .collect();
        let mut devices = Vec::new();
        for handle in handles {
            devices.push(handle.await.unwrap());
        }
        let publisher = nats().await;
        publish(&publisher, &gateway, "alice", "всем восьми").await;
        for (i, device) in devices.iter_mut().enumerate() {
            assert_eq!(next(device).await.as_deref(), Some("всем восьми"), "устройство {i} получило событие");
            assert!(silent(device).await, "устройство {i}: одна копия, а не по числу одновременных подключений");
        }
    }

    #[tokio::test]
    async fn users_do_not_hear_each_other() {
        let scope = Scope::new();
        let gateway = Gateway::new(nats().await, scope.prefix());
        let mut alice = gateway.connect("alice").await.unwrap();
        let mut bob = gateway.connect("bob").await.unwrap();
        let publisher = nats().await;
        publish(&publisher, &gateway, "bob", "для bob").await;
        assert_eq!(next(&mut bob).await.as_deref(), Some("для bob"), "bob получил");
        assert!(silent(&mut alice).await, "alice не получила чужое");
    }
}
