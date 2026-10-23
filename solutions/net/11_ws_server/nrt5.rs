// На тестовом стенде всё было быстро. В бою один пользователь зашёл в чат с телефона в метро и перестал читать сообщения.
// Через минуту у остальных не отправляются сообщения, не открываются новые соединения, а CPU шлюза при этом на нуле.
// В тестах ниже воспроизведён тот самый клиент с забитой очередью. TODO: Найди, почему встали все, и почини Hub так, чтобы
// медленный клиент вредил только себе: рассылка не ждёт его и не держит общий реестр.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

type ConnId = u64;

#[derive(Clone, Default)]
struct Hub {
    members: Arc<Mutex<HashMap<ConnId, mpsc::Sender<String>>>>,
}

impl Hub {
    async fn join(&self, id: ConnId, tx: mpsc::Sender<String>) {
        self.members.lock().await.insert(id, tx);
    }

    async fn leave(&self, id: ConnId) {
        self.members.lock().await.remove(&id);
    }

    async fn broadcast(&self, message: &str) -> usize {
        let senders: Vec<mpsc::Sender<String>> = self.members.lock().await.values().cloned().collect();
        senders.iter().filter(|tx| tx.try_send(message.to_string()).is_ok()).count()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    async fn stuck_client(hub: &Hub, id: ConnId) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(1);
        tx.try_send("старое".to_string()).unwrap();
        hub.join(id, tx).await;
        rx
    }

    #[tokio::test]
    async fn stuck_client_does_not_freeze_the_registry() {
        let hub = Hub::default();
        let _stuck = stuck_client(&hub, 1).await;
        let background = hub.clone();
        tokio::spawn(async move {
            background.broadcast("раз").await;
            background.broadcast("два").await;
        });
        sleep(Duration::from_millis(50)).await;

        let (tx, _rx) = mpsc::channel(4);
        let joined = timeout(Duration::from_millis(500), hub.join(2, tx)).await;
        assert!(joined.is_ok(), "новое соединение не смогло войти: рассылка держит замок реестра, пока ждёт медленного клиента");
        let left = timeout(Duration::from_millis(500), hub.leave(2)).await;
        assert!(left.is_ok(), "leave тоже висит на замке реестра");
    }

    #[tokio::test]
    async fn broadcast_returns_without_waiting_for_slow_clients() {
        let hub = Hub::default();
        let mut stuck = Vec::new();
        for id in 0..3 {
            stuck.push(stuck_client(&hub, id).await);
        }
        let mut fast = Vec::new();
        for id in 10..20 {
            let (tx, rx) = mpsc::channel(8);
            hub.join(id, tx).await;
            fast.push(rx);
        }
        let delivered = timeout(Duration::from_secs(1), hub.broadcast("новость")).await;
        let delivered = delivered.expect("рассылка зависла на клиенте с полной очередью");
        assert_eq!(delivered, 10, "доставлено должно быть десяти быстрым клиентам, медленные пропускаются");
        for rx in &mut fast {
            assert_eq!(rx.try_recv().as_deref(), Ok("новость"), "быстрый клиент не получил сообщение");
        }
    }

    #[tokio::test]
    async fn fast_clients_receive_even_when_slow_ones_are_everywhere() {
        let hub = Hub::default();
        let mut keep = Vec::new();
        for id in 0..40 {
            if id % 2 == 0 {
                keep.push((true, stuck_client(&hub, id).await));
            } else {
                let (tx, rx) = mpsc::channel(8);
                hub.join(id, tx).await;
                keep.push((false, rx));
            }
        }
        let delivered = timeout(Duration::from_secs(1), hub.broadcast("всем")).await.expect("рассылка зависла");
        assert_eq!(delivered, 20);
        for (stuck, rx) in &mut keep {
            if !*stuck {
                assert_eq!(rx.try_recv().as_deref(), Ok("всем"));
            }
        }
    }
}
