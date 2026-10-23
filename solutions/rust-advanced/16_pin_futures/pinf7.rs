use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct State {
    fired: bool,
    waker: Option<Waker>,
}

pub struct Timer {
    state: Arc<Mutex<State>>,
}

impl Timer {
    pub fn after(delay: Duration) -> Timer {
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        thread::spawn(move || {
            thread::sleep(delay);
            let waker = {
                let mut state = shared.lock().unwrap();
                state.fired = true;
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        Timer { state }
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.state.lock().unwrap();
        if state.fired {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn main() {
    let _timer = Timer::after(Duration::from_millis(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;
    use std::time::Instant;

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

    fn wait_for_wakes(counter: &Counter, at_least: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while counter.count() < at_least {
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(1));
        }
        true
    }

    #[test]
    fn pending_then_woken_then_ready() {
        let started = Instant::now();
        let mut timer = Timer::after(Duration::from_millis(80));
        let (counter, waker) = counter();
        assert_eq!(poll_with(&mut timer, &waker), Poll::Pending, "срок ещё не истёк");
        assert!(wait_for_wakes(&counter, 1), "таймер не разбудил опросчика");
        assert_eq!(poll_with(&mut timer, &waker), Poll::Ready(()));
        assert!(started.elapsed() >= Duration::from_millis(80), "таймер сработал раньше срока");
        assert_eq!(counter.count(), 1, "будить надо один раз");
    }

    #[test]
    fn the_clock_starts_at_creation() {
        let mut timer = Timer::after(Duration::from_millis(30));
        thread::sleep(Duration::from_millis(150));
        let (_counter, waker) = counter();
        assert_eq!(poll_with(&mut timer, &waker), Poll::Ready(()), "срок истёк до первого опроса, ждать больше нечего");
    }

    #[test]
    fn many_timers_wait_in_parallel() {
        let started = Instant::now();
        let mut timers: Vec<_> = (0..20).map(|_| Timer::after(Duration::from_millis(100))).collect();
        let wakers: Vec<_> = (0..20).map(|_| counter()).collect();
        for (timer, (_, waker)) in timers.iter_mut().zip(&wakers) {
            assert_eq!(poll_with(timer, waker), Poll::Pending);
        }
        for (counter, _) in &wakers {
            assert!(wait_for_wakes(counter, 1));
        }
        assert!(started.elapsed() < Duration::from_millis(900), "таймеры ждут по очереди, а не вместе");
    }

    #[test]
    fn the_latest_waker_is_woken() {
        let mut timer = Timer::after(Duration::from_millis(60));
        let (old, old_waker) = counter();
        let (new, new_waker) = counter();
        let _ = poll_with(&mut timer, &old_waker);
        let _ = poll_with(&mut timer, &new_waker);
        assert!(wait_for_wakes(&new, 1), "разбудить надо Waker из последнего опроса");
        assert_eq!(old.count(), 0);
    }
}
