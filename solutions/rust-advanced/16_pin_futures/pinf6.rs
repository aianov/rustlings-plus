use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Retry<F> {
    attempt: F,
    tries: u32,
}

pub fn retry<T, F: FnMut() -> Option<T>>(attempt: F) -> Retry<F> {
    Retry { attempt, tries: 0 }
}

impl<T, F: FnMut() -> Option<T> + Unpin> Future for Retry<F> {
    type Output = (T, u32);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<(T, u32)> {
        self.tries += 1;
        match (self.attempt)() {
            Some(value) => Poll::Ready((value, self.tries)),
            None => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

pub struct YieldNow {
    yielded: bool,
}

pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn main() {
    let _later = yield_now();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    struct Counter(AtomicUsize);

    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn strict<F: Future>(fut: F) -> (F::Output, usize) {
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        let mut fut = pin!(fut);
        let mut pendings = 0;
        for _ in 0..50 {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(value) => {
                    assert_eq!(counter.0.load(Ordering::SeqCst), pendings, "на каждый Pending должно быть ровно одно пробуждение");
                    return (value, pendings);
                }
                Poll::Pending => {
                    pendings += 1;
                    assert!(
                        counter.0.load(Ordering::SeqCst) >= pendings,
                        "future ответил Pending номер {pendings} и никого не разбудил: строгий опросчик заснул бы навсегда"
                    );
                }
            }
        }
        panic!("future не завершился за 50 опросов");
    }

    #[test]
    fn retry_wakes_on_every_pending() {
        let mut calls = 0;
        let ((value, tries), pendings) = strict(retry(|| {
            calls += 1;
            (calls == 4).then_some("ok")
        }));
        assert_eq!((value, tries, pendings), ("ok", 4, 3));
    }

    #[test]
    fn retry_that_succeeds_at_once_never_wakes() {
        let ((value, tries), pendings) = strict(retry(|| Some(5)));
        assert_eq!((value, tries, pendings), (5, 1, 0));
    }

    #[test]
    fn yield_now_pends_once() {
        let ((), pendings) = strict(yield_now());
        assert_eq!(pendings, 1);
    }

    #[test]
    fn yields_compose_inside_async_blocks() {
        let (value, pendings) = strict(async {
            yield_now().await;
            yield_now().await;
            yield_now().await;
            7
        });
        assert_eq!((value, pendings), (7, 3));
    }
}
