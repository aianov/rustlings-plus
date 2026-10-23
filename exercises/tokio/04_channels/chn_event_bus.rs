// Шина событий нашей соцсети. Ядро (одна фоновая задача) принимает команды и рассылает события,
// а остальные части приложения пользуются дескриптором `Bus`.
//
// Требования:
//  1. Команды идут в ядро через ограниченную очередь (`Config::queue` мест). `publish` ждёт, пока
//     в очереди появится место, и получает ответ от ядра: номер поста (1, 2, 3 ...) или ошибку
//     TooLong, если текст длиннее текущего лимита (считай символы). Отклонённый пост номер не тратит
//     и события не порождает.
//  2. `try_follow` не ждёт никогда: ставит подписку в очередь или сразу отвечает Overloaded
//     (очередь полна), Closed (ядра нет).
//  3. Каждая принятая команда порождает событие для всех подписчиков (посты NewPost, подписки
//     NewFollower), событие одно на всех, а не копия на каждого (`Arc<Event>`). Подписчиков может
//     не быть вовсе, это не ошибка. Буфер событий `Config::events`, отстающий подписчик узнаёт о
//     потерях: `Subscription::next` отдаёт Next::Missed(n), а потом продолжает с оставшегося.
//  4. Лимит длины живой: `set_max_len` меняет его для следующих постов, без перезапуска; ядро читает
//     лимит в момент обработки поста. `max_len` возвращает текущий.
//  5. `shutdown` гасит шину: ядро обрабатывает всё, что уже в очереди, и возвращает статистику
//     (принятые посты и подписки). После этого подписчики дочитывают буфер и получают Next::Closed.
//  Ядро тратит на каждую команду `Config::work` (изображает работу).
// TODO: Допиши `Bus`, `Subscription` и ядро. Команды и поля придумай сам.
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

pub struct Bus {}

pub struct Subscription {}

impl Bus {
    pub fn start(config: Config) -> Bus {
        todo!()
    }

    pub async fn publish(&self, author: &str, text: &str) -> Result<u64, BusError> {
        todo!()
    }

    pub fn try_follow(&self, follower: &str, target: &str) -> Result<(), BusError> {
        todo!()
    }

    pub fn subscribe(&self) -> Subscription {
        todo!()
    }

    pub fn set_max_len(&self, max_len: usize) {
        todo!()
    }

    pub fn max_len(&self) -> usize {
        todo!()
    }

    pub async fn shutdown(self) -> Stats {
        todo!()
    }
}

impl Subscription {
    pub async fn next(&mut self) -> Next {
        todo!()
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
