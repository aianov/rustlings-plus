use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct Shared {
    items: VecDeque<u32>,
    closed: bool,
    waker: Option<Waker>,
}

#[derive(Clone, Default)]
struct Queue(Arc<Mutex<Shared>>);

impl Queue {
    fn push(&self, item: u32) {
        let mut shared = self.0.lock().unwrap();
        shared.items.push_back(item);
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }

    fn close(&self) {
        let mut shared = self.0.lock().unwrap();
        shared.closed = true;
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl Stream for Queue {
    type Item = u32;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<u32>> {
        let mut shared = self.0.lock().unwrap();
        if let Some(item) = shared.items.pop_front() {
            return Poll::Ready(Some(item));
        }
        if shared.closed {
            return Poll::Ready(None);
        }
        shared.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let queue = Queue::default();
    queue.push(1);
    queue.close();
    println!("{:?}", queue.collect::<Vec<_>>().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn consumer_wakes_up_for_late_items() {
        let queue = Queue::default();
        let producer = queue.clone();
        tokio::spawn(async move {
            for n in 1..=3 {
                sleep(Duration::from_millis(20)).await;
                producer.push(n);
            }
            producer.close();
        });
        let consumer = tokio::spawn(queue.collect::<Vec<_>>());
        let got = timeout(Duration::from_secs(5), consumer).await;
        let got = got.expect("поток завис: потребитель ждёт, а его никто не разбудил").unwrap();
        assert_eq!(got, vec![1, 2, 3], "все события должны дойти по порядку");
    }

    #[tokio::test]
    async fn ready_items_are_returned_without_waiting() {
        let queue = Queue::default();
        queue.push(7);
        queue.push(8);
        queue.close();
        assert_eq!(queue.collect::<Vec<_>>().await, vec![7, 8], "готовые события отдаются сразу, закрытая очередь заканчивается");
    }

    #[tokio::test(start_paused = true)]
    async fn close_wakes_a_waiting_consumer() {
        let queue = Queue::default();
        let closer = queue.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            closer.close();
        });
        let consumer = tokio::spawn(queue.collect::<Vec<_>>());
        let got = timeout(Duration::from_secs(5), consumer).await;
        assert!(got.expect("закрытие очереди не разбудило потребителя").unwrap().is_empty(), "событий не было, поток пуст");
    }
}
