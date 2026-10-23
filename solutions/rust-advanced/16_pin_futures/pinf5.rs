use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct State {
    is_set: bool,
    waker: Option<Waker>,
}

#[derive(Clone, Default)]
pub struct Signal {
    state: Arc<Mutex<State>>,
}

pub struct Wait {
    state: Arc<Mutex<State>>,
}

impl Signal {
    pub fn new() -> Signal {
        Signal::default()
    }

    pub fn wait(&self) -> Wait {
        Wait { state: Arc::clone(&self.state) }
    }

    pub fn set(&self) {
        let waker = {
            let mut state = self.state.lock().unwrap();
            state.is_set = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.state.lock().unwrap();
        if state.is_set {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn main() {
    let signal = Signal::new();
    signal.set();
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

    impl Counter {
        fn count(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn counter() -> (Arc<Counter>, Waker) {
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        (counter, waker)
    }

    fn poll_with<F: Future + Unpin>(fut: &mut F, waker: &Waker) -> Poll<F::Output> {
        Pin::new(fut).poll(&mut Context::from_waker(waker))
    }

    #[test]
    fn pending_until_set_then_exactly_one_wake() {
        let signal = Signal::new();
        let mut wait = signal.wait();
        let (counter, waker) = counter();
        assert_eq!(poll_with(&mut wait, &waker), Poll::Pending);
        assert_eq!(counter.count(), 0, "сигнала ещё нет, будить рано");
        signal.set();
        assert_eq!(counter.count(), 1, "set должен разбудить ожидающего через сохранённый Waker");
        assert_eq!(poll_with(&mut wait, &waker), Poll::Ready(()));
    }

    #[test]
    fn a_signal_set_before_the_first_poll_needs_no_wake() {
        let signal = Signal::new();
        signal.set();
        let mut wait = signal.wait();
        let (counter, waker) = counter();
        assert_eq!(poll_with(&mut wait, &waker), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn the_latest_waker_is_the_one_that_gets_woken() {
        let signal = Signal::new();
        let mut wait = signal.wait();
        let (old, old_waker) = counter();
        let (new, new_waker) = counter();
        assert_eq!(poll_with(&mut wait, &old_waker), Poll::Pending);
        assert_eq!(poll_with(&mut wait, &new_waker), Poll::Pending);
        signal.set();
        assert_eq!(new.count(), 1, "задачу переложили, и будить надо Waker из последнего опроса");
        assert_eq!(old.count(), 0, "старый Waker уже не нужен");
    }

    #[test]
    fn a_waker_is_used_only_once() {
        let signal = Signal::new();
        let mut wait = signal.wait();
        let (counter, waker) = counter();
        let _ = poll_with(&mut wait, &waker);
        signal.set();
        signal.set();
        assert_eq!(counter.count(), 1, "после пробуждения Waker отдан, второй set будить некого");
    }

    #[test]
    fn wakes_from_another_thread() {
        let signal = Signal::new();
        let mut wait = signal.wait();
        let (counter, waker) = counter();
        let _ = poll_with(&mut wait, &waker);
        let remote = signal.clone();
        std::thread::spawn(move || remote.set()).join().unwrap();
        assert_eq!(counter.count(), 1);
    }
}
