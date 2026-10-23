// Очередь с ожиданием элемента: `pop_wait` возвращает элемент, а если очередь пуста, ждёт, пока
// появится. `push` будит всех ждущих (notify_waiters). Чтение очереди (`peek_pop`) здесь медленное
// и содержит точку ожидания, как чтение из внешнего хранилища. Если элемент положили в промежутке
// между «очередь пуста» и «начал ждать», пробуждение пропадает, и потребитель спит рядом с
// готовым элементом.
// TODO: Допиши `pop_wait` так, чтобы такого промежутка не было.
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::Notify;

struct Queue {
    items: Mutex<VecDeque<u32>>,
    notify: Notify,
}

impl Queue {
    fn new() -> Self {
        Queue { items: Mutex::new(VecDeque::new()), notify: Notify::new() }
    }

    fn push(&self, value: u32) {
        self.items.lock().unwrap().push_back(value);
        self.notify.notify_waiters();
    }

    async fn peek_pop(&self) -> Option<u32> {
        let value = self.items.lock().unwrap().pop_front();
        tokio::task::yield_now().await;
        value
    }
}

impl Queue {
    async fn pop_wait(&self) -> u32 {
        loop {
            if let Some(value) = self.peek_pop().await {
                return value;
            }
            self.notify.notified().await;
        }
    }
}

#[tokio::main]
async fn main() {
    let queue = Queue::new();
    queue.push(1);
    println!("{}", queue.pop_wait().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn item_pushed_during_the_check_is_not_missed() {
        let queue = Arc::new(Queue::new());
        let consumer = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.pop_wait().await })
        };
        tokio::task::yield_now().await;
        queue.push(7);
        let got = timeout(Duration::from_secs(1), consumer)
            .await
            .expect("потребитель проспал элемент: пробуждение пришло в промежутке между проверкой и ожиданием")
            .unwrap();
        assert_eq!(got, 7, "потребитель получает элемент");
    }

    #[tokio::test(start_paused = true)]
    async fn ready_item_is_returned_at_once() {
        let queue = Queue::new();
        queue.push(3);
        assert_eq!(timeout(Duration::from_secs(1), queue.pop_wait()).await.unwrap(), 3, "готовый элемент отдаётся сразу");
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_consumer_wakes_on_push() {
        let queue = Arc::new(Queue::new());
        let consumer = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.pop_wait().await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        queue.push(9);
        assert_eq!(timeout(Duration::from_secs(1), consumer).await.unwrap().unwrap(), 9, "спящий потребитель просыпается от push");
    }

    #[tokio::test(start_paused = true)]
    async fn two_consumers_get_all_items_once() {
        let queue = Arc::new(Queue::new());
        let spawn_consumer = |queue: Arc<Queue>| {
            tokio::spawn(async move {
                let mut got = Vec::new();
                for _ in 0..2 {
                    got.push(queue.pop_wait().await);
                }
                got
            })
        };
        let a = spawn_consumer(Arc::clone(&queue));
        let b = spawn_consumer(Arc::clone(&queue));
        for value in 1..=4 {
            tokio::task::yield_now().await;
            queue.push(value);
        }
        let mut all = Vec::new();
        all.extend(timeout(Duration::from_secs(1), a).await.expect("первый потребитель завис").unwrap());
        all.extend(timeout(Duration::from_secs(1), b).await.expect("второй потребитель завис").unwrap());
        all.sort();
        assert_eq!(all, [1, 2, 3, 4], "каждый элемент достаётся ровно одному потребителю");
    }
}
