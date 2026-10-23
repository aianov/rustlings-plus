// Главная страница: рекомендации нужны всем, а счётчик непрочитанных только вошедшему пользователю.
// Рекомендации считаются долго (80 мс), счётчик тоже небыстрый (50 мс). Ждать их по очереди нельзя,
// а гостю ходить за счётчиком незачем: у него его нет, и запрос стоит денег.
// TODO: Допиши `home`.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

struct Store {
    unread_calls: AtomicUsize,
}

impl Store {
    async fn recommendations(&self) -> Vec<u32> {
        sleep(Duration::from_millis(80)).await;
        vec![3, 1, 4]
    }

    async fn unread(&self, user: u32) -> u32 {
        self.unread_calls.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;
        user + 5
    }
}

#[derive(Debug, PartialEq)]
struct Home {
    recommendations: Vec<u32>,
    unread: Option<u32>,
}

async fn home(store: &Store, viewer: Option<u32>) -> Home {
    todo!()
}

#[tokio::main]
async fn main() {
    let store = Store { unread_calls: AtomicUsize::new(0) };
    println!("{:?}", home(&store, Some(7)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn member_gets_both_in_parallel() {
        let store = Store { unread_calls: AtomicUsize::new(0) };
        let start = Instant::now();
        let page = home(&store, Some(7)).await;
        assert_eq!(page, Home { recommendations: vec![3, 1, 4], unread: Some(12) }, "у вошедшего есть и рекомендации, и счётчик");
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(80),
            "рекомендации и счётчик должны идти одновременно: ждём 80 мс, а не 130"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn guest_does_not_pay_for_the_counter() {
        let store = Store { unread_calls: AtomicUsize::new(0) };
        let start = Instant::now();
        let page = home(&store, None).await;
        assert_eq!(page, Home { recommendations: vec![3, 1, 4], unread: None }, "у гостя счётчика нет");
        assert_eq!(store.unread_calls.load(Ordering::SeqCst), 0, "для гостя запрос счётчика делать нельзя");
        assert_eq!(start.elapsed(), Duration::from_millis(80), "гость ждёт только рекомендации");
    }

    #[tokio::test(start_paused = true)]
    async fn counter_is_asked_exactly_once() {
        let store = Store { unread_calls: AtomicUsize::new(0) };
        home(&store, Some(1)).await;
        assert_eq!(store.unread_calls.load(Ordering::SeqCst), 1, "вошедший пользователь стоит ровно одного запроса счётчика");
    }
}
