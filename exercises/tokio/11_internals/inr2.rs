// Одноразовый канал значения, написанный вручную: получатель это Future, отправитель будит его.
// Задача переезжает между обработчиками (так делает, например, `select!` в другой ветке), и
// значение, которое уже отправлено, не доходит: будят не того, кто сейчас ждёт, а получатель висит.
// Разберись, почему, и добейся, чтобы просыпался тот, кто опрашивал последним.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

struct Shared<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

struct Sender<T>(Arc<Mutex<Shared<T>>>);
struct Receiver<T>(Arc<Mutex<Shared<T>>>);

fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Mutex::new(Shared { value: None, waker: None }));
    (Sender(shared.clone()), Receiver(shared))
}

impl<T> Sender<T> {
    fn send(self, value: T) {
        let mut shared = self.0.lock().unwrap();
        shared.value = Some(value);
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut shared = self.0.lock().unwrap();
        if let Some(value) = shared.value.take() {
            return Poll::Ready(value);
        }
        if shared.waker.is_none() {
            shared.waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

fn main() {
    let (tx, rx) = channel::<u32>();
    tx.send(1);
    drop(rx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;

    struct Counter(AtomicUsize);

    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn waker() -> (Waker, Arc<Counter>) {
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        (Waker::from(counter.clone()), counter)
    }

    fn woken(counter: &Arc<Counter>) -> usize {
        counter.0.load(Ordering::SeqCst)
    }

    #[test]
    fn the_value_wakes_the_waiter_once() {
        let (tx, mut rx) = channel::<u32>();
        let (waker, counter) = waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Pending, "значения ещё нет: Pending");
        tx.send(7);
        assert_eq!(woken(&counter), 1, "отправка должна разбудить ждущего ровно один раз");
        assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Ready(7), "после пробуждения значение забирается");
    }

    #[test]
    fn repeated_polls_with_the_same_waker_wake_once() {
        let (tx, mut rx) = channel::<u32>();
        let (waker, counter) = waker();
        let mut cx = Context::from_waker(&waker);
        for _ in 0..3 {
            assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Pending);
        }
        tx.send(1);
        assert_eq!(woken(&counter), 1, "один ждущий: одно пробуждение, сколько бы раз его ни опрашивали");
    }

    #[test]
    fn the_task_that_polled_last_is_the_one_who_wakes() {
        let (tx, mut rx) = channel::<u32>();
        let (first, first_count) = waker();
        let (second, second_count) = waker();
        assert_eq!(Pin::new(&mut rx).poll(&mut Context::from_waker(&first)), Poll::Pending);
        assert_eq!(Pin::new(&mut rx).poll(&mut Context::from_waker(&second)), Poll::Pending);
        tx.send(5);
        assert_eq!(woken(&second_count), 1, "получатель теперь опрашивается из второй задачи: будить надо её");
        assert_eq!(woken(&first_count), 0, "первая задача получателя больше не ждёт, будить её незачем");
        assert_eq!(Pin::new(&mut rx).poll(&mut Context::from_waker(&second)), Poll::Ready(5));
    }

    #[test]
    fn a_value_sent_before_the_first_poll_is_ready_at_once() {
        let (tx, mut rx) = channel::<&str>();
        tx.send("привет");
        let (waker, counter) = waker();
        assert_eq!(Pin::new(&mut rx).poll(&mut Context::from_waker(&waker)), Poll::Ready("привет"), "значение уже лежит: ждать нечего");
        assert_eq!(woken(&counter), 0, "будить некого, никто не ждал");
    }
}
