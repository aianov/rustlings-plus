#![allow(dead_code)]
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    NewPost { id: u64, author: String, text: String },
    NewFollower { follower: String, target: String },
}

#[derive(Debug, PartialEq)]
pub enum BusError {
    TooLong { max: usize },
    Overloaded,
    Closed,
}

#[derive(Debug, PartialEq)]
pub enum Next {
    Event(Arc<Event>),
    Missed(u64),
    Closed,
}

#[derive(Debug, Default, PartialEq)]
pub struct Stats {
    pub posts: u64,
    pub follows: u64,
}

#[derive(Clone)]
pub struct Config {
    pub queue: usize,
    pub events: usize,
    pub max_len: usize,
    pub work: Duration,
}

enum Command {
    Publish { author: String, text: String, reply: oneshot::Sender<Result<u64, BusError>> },
    Follow { follower: String, target: String },
}

pub struct Bus {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<Arc<Event>>,
    max_len: watch::Sender<usize>,
    core: JoinHandle<Stats>,
}

pub struct Subscription {
    rx: broadcast::Receiver<Arc<Event>>,
}

async fn run_core(
    mut commands: mpsc::Receiver<Command>,
    events: broadcast::Sender<Arc<Event>>,
    max_len: watch::Receiver<usize>,
    work: Duration,
) -> Stats {
    let mut stats = Stats::default();
    while let Some(command) = commands.recv().await {
        tokio::time::sleep(work).await;
        match command {
            Command::Publish { author, text, reply } => {
                let max = *max_len.borrow();
                if text.chars().count() > max {
                    let _ = reply.send(Err(BusError::TooLong { max }));
                    continue;
                }
                stats.posts += 1;
                let id = stats.posts;
                let _ = events.send(Arc::new(Event::NewPost { id, author, text }));
                let _ = reply.send(Ok(id));
            }
            Command::Follow { follower, target } => {
                stats.follows += 1;
                let _ = events.send(Arc::new(Event::NewFollower { follower, target }));
            }
        }
    }
    stats
}

impl Bus {
    pub fn start(config: Config) -> Bus {
        let (commands, rx) = mpsc::channel(config.queue);
        let (events, _) = broadcast::channel(config.events);
        let (max_len, max_len_rx) = watch::channel(config.max_len);
        let core = tokio::spawn(run_core(rx, events.clone(), max_len_rx, config.work));
        Bus { commands, events, max_len, core }
    }

    pub async fn publish(&self, author: &str, text: &str) -> Result<u64, BusError> {
        let (reply, answer) = oneshot::channel();
        let command = Command::Publish { author: author.to_string(), text: text.to_string(), reply };
        self.commands.send(command).await.map_err(|_| BusError::Closed)?;
        answer.await.map_err(|_| BusError::Closed)?
    }

    pub fn try_follow(&self, follower: &str, target: &str) -> Result<(), BusError> {
        let command = Command::Follow { follower: follower.to_string(), target: target.to_string() };
        self.commands.try_send(command).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => BusError::Overloaded,
            mpsc::error::TrySendError::Closed(_) => BusError::Closed,
        })
    }

    pub fn subscribe(&self) -> Subscription {
        Subscription { rx: self.events.subscribe() }
    }

    pub fn set_max_len(&self, max_len: usize) {
        self.max_len.send_replace(max_len);
    }

    pub fn max_len(&self) -> usize {
        *self.max_len.borrow()
    }

    pub async fn shutdown(self) -> Stats {
        let Bus { commands, core, .. } = self;
        drop(commands);
        core.await.unwrap_or_default()
    }
}

impl Subscription {
    pub async fn next(&mut self) -> Next {
        match self.rx.recv().await {
            Ok(event) => Next::Event(event),
            Err(broadcast::error::RecvError::Lagged(n)) => Next::Missed(n),
            Err(broadcast::error::RecvError::Closed) => Next::Closed,
        }
    }
}

#[tokio::main]
async fn main() {
    let bus = Bus::start(Config { queue: 8, events: 16, max_len: 280, work: Duration::ZERO });
    let mut sub = bus.subscribe();
    println!("{:?}", bus.publish("ann", "привет").await);
    println!("{:?}", sub.next().await);
    println!("{:?}", bus.shutdown().await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(queue: usize, events: usize, max_len: usize, work_ms: u64) -> Config {
        Config { queue, events, max_len, work: Duration::from_millis(work_ms) }
    }

    fn post(id: u64, author: &str, text: &str) -> Next {
        Next::Event(Arc::new(Event::NewPost { id, author: author.into(), text: text.into() }))
    }

    #[tokio::test(start_paused = true)]
    async fn commands_become_events_for_everyone_in_order() {
        let bus = Bus::start(config(8, 16, 100, 0));
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();
        assert_eq!(bus.publish("ann", "привет").await, Ok(1), "первый пост получает номер 1");
        assert_eq!(bus.publish("bob", "и тебе").await, Ok(2), "второй пост получает номер 2");
        assert_eq!(bus.try_follow("bob", "ann"), Ok(()), "подписка встала в очередь");
        let stats = bus.shutdown().await;
        assert_eq!(stats, Stats { posts: 2, follows: 1 }, "статистика после остановки");
        for sub in [&mut first, &mut second] {
            assert_eq!(sub.next().await, post(1, "ann", "привет"), "первое событие: пост 1");
            assert_eq!(sub.next().await, post(2, "bob", "и тебе"), "второе событие: пост 2");
            assert_eq!(
                sub.next().await,
                Next::Event(Arc::new(Event::NewFollower { follower: "bob".into(), target: "ann".into() })),
                "третье событие: подписка"
            );
            assert_eq!(sub.next().await, Next::Closed, "после остановки и дочитанного буфера канал закрыт");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn works_without_any_subscribers() {
        let bus = Bus::start(config(8, 16, 100, 0));
        assert_eq!(bus.publish("ann", "никто не слушает").await, Ok(1), "отсутствие подписчиков не ошибка");
        assert_eq!(bus.shutdown().await.posts, 1, "пост принят");
    }

    #[tokio::test(start_paused = true)]
    async fn too_long_post_is_rejected_without_a_trace() {
        let bus = Bus::start(config(8, 16, 5, 0));
        let mut sub = bus.subscribe();
        assert_eq!(bus.publish("ann", "слишком длинный").await, Err(BusError::TooLong { max: 5 }), "лимит 5 символов");
        assert_eq!(bus.publish("ann", "пять!").await, Ok(1), "отказ не должен тратить номер поста");
        bus.shutdown().await;
        assert_eq!(sub.next().await, post(1, "ann", "пять!"), "в событиях только принятый пост");
        assert_eq!(sub.next().await, Next::Closed, "отклонённый пост событий не порождает");
    }

    #[tokio::test(start_paused = true)]
    async fn limit_is_live() {
        let bus = Bus::start(config(8, 16, 5, 0));
        assert_eq!(bus.max_len(), 5, "стартовый лимит");
        assert!(bus.publish("ann", "шесть!").await.is_err(), "шесть символов при лимите 5");
        bus.set_max_len(10);
        assert_eq!(bus.max_len(), 10, "лимит обновился");
        assert_eq!(bus.publish("ann", "шесть!").await, Ok(1), "новый лимит действует без перезапуска");
        bus.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn lagging_subscriber_learns_about_losses() {
        let bus = Bus::start(config(8, 4, 100, 0));
        let mut slow = bus.subscribe();
        for i in 1..=10u64 {
            bus.publish("ann", &format!("пост {i}")).await.unwrap();
        }
        bus.shutdown().await;
        assert_eq!(slow.next().await, Next::Missed(6), "буфер на четыре места, шесть событий затёрты");
        for i in 7..=10u64 {
            assert_eq!(slow.next().await, post(i, "ann", &format!("пост {i}")), "после сообщения о потере идут оставшиеся");
        }
        assert_eq!(slow.next().await, Next::Closed, "и затем конец");
    }

    #[tokio::test(start_paused = true)]
    async fn try_follow_never_waits_for_a_busy_core() {
        let bus = Bus::start(config(2, 16, 100, 100));
        assert_eq!(bus.try_follow("a", "x"), Ok(()), "первая команда принята");
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(bus.try_follow("b", "x"), Ok(()), "ядро занято первой, но в очереди есть место");
        assert_eq!(bus.try_follow("c", "x"), Ok(()), "второе место в очереди");
        assert_eq!(bus.try_follow("d", "x"), Err(BusError::Overloaded), "очередь на два места полна");
        let stats = bus.shutdown().await;
        assert_eq!(stats.follows, 3, "четвёртая подписка отброшена и не выполняется");
    }
}
