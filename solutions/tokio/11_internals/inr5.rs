use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

type Token = usize;

#[derive(Default)]
struct Slot {
    ready: bool,
    waker: Option<Waker>,
}

#[derive(Clone, Default)]
struct Reactor {
    slots: Arc<Mutex<HashMap<Token, Slot>>>,
}

struct Readable {
    reactor: Reactor,
    token: Token,
}

impl Reactor {
    fn register(&self, token: Token) {
        self.slots.lock().unwrap().insert(token, Slot::default());
    }

    fn readable(&self, token: Token) -> Readable {
        Readable { reactor: self.clone(), token }
    }

    fn notify(&self, token: Token) {
        let mut slots = self.slots.lock().unwrap();
        if let Some(slot) = slots.get_mut(&token) {
            slot.ready = true;
            if let Some(waker) = slot.waker.take() {
                waker.wake();
            }
        }
    }
}

impl Future for Readable {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut slots = self.reactor.slots.lock().unwrap();
        let slot = slots.get_mut(&self.token).expect("токен не зарегистрирован");
        if slot.ready {
            slot.ready = false;
            return Poll::Ready(());
        }
        slot.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

fn main() {
    let reactor = Reactor::default();
    reactor.register(1);
    reactor.notify(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn an_event_wakes_a_waiting_task() {
        let reactor = Reactor::default();
        reactor.register(1);
        let waiting = tokio::spawn(reactor.readable(1));
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        reactor.notify(1);
        timeout(Duration::from_secs(1), waiting).await.expect("событие пришло, а ждущий не проснулся").unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn an_event_before_the_first_poll_is_not_lost() {
        let reactor = Reactor::default();
        reactor.register(1);
        reactor.notify(1);
        timeout(Duration::from_secs(1), reactor.readable(1))
            .await
            .expect("событие пришло раньше, чем задача встала ждать: готовность должна запомниться");
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_events_make_one_readiness() {
        let reactor = Reactor::default();
        reactor.register(1);
        reactor.notify(1);
        reactor.notify(1);
        timeout(Duration::from_secs(1), reactor.readable(1)).await.expect("готовность одна, но она есть");
        let second = timeout(Duration::from_millis(100), reactor.readable(1)).await;
        assert!(second.is_err(), "готовность забрана: пока нового события нет, второе ожидание должно ждать");
    }

    #[tokio::test(start_paused = true)]
    async fn tokens_do_not_mix() {
        let reactor = Reactor::default();
        reactor.register(1);
        reactor.register(2);
        reactor.notify(2);
        assert!(timeout(Duration::from_millis(100), reactor.readable(1)).await.is_err(), "событие на токене 2 не касается токена 1");
        timeout(Duration::from_secs(1), reactor.readable(2)).await.expect("токен 2 готов");
    }

    #[tokio::test]
    async fn a_device_thread_racing_with_the_task_never_loses_an_event() {
        let reactor = Reactor::default();
        for token in 0..200 {
            reactor.register(token);
            let r = reactor.clone();
            let thread = std::thread::Builder::new().spawn(move || r.notify(token)).unwrap();
            if token % 2 == 0 {
                sleep(Duration::from_micros(50)).await;
            }
            timeout(Duration::from_secs(5), reactor.readable(token))
                .await
                .unwrap_or_else(|_| panic!("токен {token}: событие потеряно в гонке с постановкой на ожидание"));
            thread.join().unwrap();
        }
    }
}
