
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialError(pub String);

pub trait Dialer: Send + Sync {
    fn dial(&self, addr: &str) -> BoxFuture<'_, Result<Channel, DialError>>;
}

pub struct Channels {
    dialer: Arc<dyn Dialer>,
    cells: Mutex<HashMap<String, Arc<OnceCell<Channel>>>>,
}

impl Channels {
    pub fn new(dialer: Arc<dyn Dialer>) -> Channels {
        Channels { dialer, cells: Mutex::new(HashMap::new()) }
    }

    pub async fn get(&self, addr: &str) -> Result<Channel, DialError> {
        let cell = self.cells.lock().unwrap().entry(addr.to_string()).or_default().clone();
        cell.get_or_try_init(|| self.dialer.dial(addr)).await.cloned()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::Instant;

    struct Fake {
        dials: AtomicU64,
        fail_first: AtomicUsize,
    }

    impl Dialer for Fake {
        fn dial(&self, addr: &str) -> BoxFuture<'_, Result<Channel, DialError>> {
            let delay = if addr == "slow:1" { 100 } else { 10 };
            async move {
                let id = self.dials.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::time::sleep(Duration::from_millis(delay)).await;
                if self.fail_first.load(Ordering::SeqCst) > 0 {
                    self.fail_first.fetch_sub(1, Ordering::SeqCst);
                    return Err(DialError("connection refused".to_string()));
                }
                Ok(Channel { id })
            }
            .boxed()
        }
    }

    fn setup(fail_first: usize) -> (Arc<Channels>, Arc<Fake>) {
        let fake = Arc::new(Fake { dials: AtomicU64::new(0), fail_first: AtomicUsize::new(fail_first) });
        (Arc::new(Channels::new(fake.clone())), fake)
    }

    #[tokio::test(start_paused = true)]
    async fn one_channel_per_address_is_reused() {
        let (channels, fake) = setup(0);
        let first = channels.get("user:50051").await.unwrap();
        for _ in 0..5 {
            assert_eq!(channels.get("user:50051").await.unwrap(), first, "тот же канал");
        }
        assert_eq!(fake.dials.load(Ordering::SeqCst), 1, "соединение открыто один раз");
    }

    #[tokio::test(start_paused = true)]
    async fn simultaneous_first_calls_dial_once() {
        let (channels, fake) = setup(0);
        let calls: Vec<_> = (0..10)
            .map(|_| {
                let channels = channels.clone();
                tokio::spawn(async move { channels.get("user:50051").await })
            })
            .collect();
        let mut ids = Vec::new();
        for call in calls {
            ids.push(call.await.unwrap().unwrap().id);
        }
        assert_eq!(fake.dials.load(Ordering::SeqCst), 1, "десять одновременных вызовов открыли {} соединений", fake.dials.load(Ordering::SeqCst));
        assert!(ids.iter().all(|id| *id == ids[0]), "все получили один канал");
    }

    #[tokio::test(start_paused = true)]
    async fn failure_is_not_remembered() {
        let (channels, fake) = setup(1);
        assert_eq!(channels.get("user:50051").await, Err(DialError("connection refused".to_string())), "первая попытка не удалась");
        let channel = channels.get("user:50051").await.expect("следующая попытка пробует снова");
        assert_eq!(channel.id, 2, "это новое соединение");
        assert_eq!(channels.get("user:50051").await.unwrap(), channel, "а теперь канал запомнен");
        assert_eq!(fake.dials.load(Ordering::SeqCst), 2, "две попытки в сумме");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_address_does_not_block_another() {
        let (channels, _) = setup(0);
        let started = Instant::now();
        let slow = {
            let channels = channels.clone();
            tokio::spawn(async move { channels.get("slow:1").await })
        };
        tokio::time::sleep(Duration::from_millis(1)).await;
        channels.get("user:50051").await.unwrap();
        assert_eq!(started.elapsed(), Duration::from_millis(11), "быстрый адрес ответил за свои 10 мс, не дожидаясь медленного");
        slow.await.unwrap().unwrap();
        assert_eq!(started.elapsed(), Duration::from_millis(100), "медленный закончил в своё время");
    }

    #[tokio::test(start_paused = true)]
    async fn different_addresses_get_different_channels() {
        let (channels, fake) = setup(0);
        let a = channels.get("a:1").await.unwrap();
        let b = channels.get("b:1").await.unwrap();
        assert_ne!(a, b, "разные адреса, разные каналы");
        assert_eq!(fake.dials.load(Ordering::SeqCst), 2, "по одному на адрес");
    }
}
