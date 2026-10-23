// «Калитка»: `gate()` отдаёт пару из ожидающего `Gate` и `Opener`; один поток открывает калитку, другой
// её ждёт. На проде ожидающие иногда зависают навсегда, хотя калитку точно открыли. Воспроизвести
// это сложно, потому что окно крошечное, поэтому в `poll` оставлена пауза: именно столько могла
// занимать отладочная печать между проверкой и сохранением Waker. Если калитка открылась в эту паузу,
// пробуждение теряется.
// TODO: Почини `Gate::poll`, чтобы открытие в любой момент либо замечалось сразу, либо будило.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct State {
    open: bool,
    waker: Option<Waker>,
}

pub struct Gate {
    state: Arc<Mutex<State>>,
}

pub struct Opener {
    state: Arc<Mutex<State>>,
}

pub fn gate() -> (Gate, Opener) {
    let state = Arc::new(Mutex::new(State::default()));
    (Gate { state: Arc::clone(&state) }, Opener { state })
}

impl Opener {
    pub fn open(&self) {
        let waker = {
            let mut state = self.state.lock().unwrap();
            state.open = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for Gate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let open = self.state.lock().unwrap().open;
        if open {
            return Poll::Ready(());
        }
        thread::sleep(Duration::from_millis(30));
        self.state.lock().unwrap().waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

fn main() {
    let (_gate, opener) = gate();
    opener.open();
    thread::sleep(Duration::from_millis(1));
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
        let deadline = Instant::now() + Duration::from_secs(2);
        while counter.count() < at_least {
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(1));
        }
        true
    }

    #[test]
    fn opening_during_the_poll_is_not_lost() {
        let (mut gate, opener) = gate();
        let (counter, waker) = counter();
        let remote = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            opener.open();
        });
        if poll_with(&mut gate, &waker).is_pending() {
            assert!(wait_for_wakes(&counter, 1), "калитку открыли во время опроса, а Gate не проснулся: пробуждение потеряно");
            assert_eq!(poll_with(&mut gate, &waker), Poll::Ready(()));
        }
        remote.join().unwrap();
    }

    #[test]
    fn opening_after_pending_wakes_once() {
        let (mut gate, opener) = gate();
        let (counter, waker) = counter();
        let first = poll_with(&mut gate, &waker);
        if first.is_pending() {
            opener.open();
            assert!(wait_for_wakes(&counter, 1));
            assert_eq!(poll_with(&mut gate, &waker), Poll::Ready(()));
        }
        assert!(counter.count() <= 1, "лишних пробуждений быть не должно");
    }

    #[test]
    fn an_open_gate_is_ready_at_once() {
        let (mut gate, opener) = gate();
        opener.open();
        let (counter, waker) = counter();
        assert_eq!(poll_with(&mut gate, &waker), Poll::Ready(()));
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn the_race_does_not_show_up_over_many_rounds() {
        for round in 0..10 {
            let (mut gate, opener) = gate();
            let (counter, waker) = counter();
            let remote = thread::spawn(move || {
                thread::sleep(Duration::from_millis(round % 4));
                opener.open();
            });
            let mut done = poll_with(&mut gate, &waker).is_ready();
            while !done {
                assert!(wait_for_wakes(&counter, 1), "раунд {round}: потеряно пробуждение");
                done = poll_with(&mut gate, &waker).is_ready();
                if !done {
                    assert!(wait_for_wakes(&counter, 2), "раунд {round}: потеряно второе пробуждение");
                }
            }
            remote.join().unwrap();
        }
    }
}
