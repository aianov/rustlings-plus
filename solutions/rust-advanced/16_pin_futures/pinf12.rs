use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

type BoxFut<T> = Pin<Box<dyn Future<Output = T>>>;

enum Slot<T> {
    Running(BoxFut<T>),
    Done(Option<T>),
}

pub struct Join2<A, B> {
    a: Slot<A>,
    b: Slot<B>,
}

pub fn join2<A: 'static, B: 'static>(a: impl Future<Output = A> + 'static, b: impl Future<Output = B> + 'static) -> Join2<A, B> {
    Join2 { a: Slot::Running(Box::pin(a)), b: Slot::Running(Box::pin(b)) }
}

impl<T> Slot<T> {
    fn advance(&mut self, cx: &mut Context<'_>) {
        if let Slot::Running(fut) = self {
            if let Poll::Ready(value) = fut.as_mut().poll(cx) {
                *self = Slot::Done(Some(value));
            }
        }
    }

    fn take(&mut self) -> Option<T> {
        match self {
            Slot::Done(value) => value.take(),
            Slot::Running(_) => None,
        }
    }
}

impl<A: Unpin, B: Unpin> Future for Join2<A, B> {
    type Output = (A, B);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<(A, B)> {
        let this = self.get_mut();
        this.a.advance(cx);
        this.b.advance(cx);
        if matches!(this.a, Slot::Done(_)) && matches!(this.b, Slot::Done(_)) {
            let a = this.a.take().expect("Join2 опрошен после Ready");
            let b = this.b.take().expect("Join2 опрошен после Ready");
            Poll::Ready((a, b))
        } else {
            Poll::Pending
        }
    }
}

fn main() {
    let _joined = join2(async { 1 }, async { "x" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::pin::pin;
    use std::rc::Rc;
    use std::task::Waker;

    struct Yield(bool);

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    struct Probe<F> {
        inner: Pin<Box<F>>,
        polls: Rc<Cell<u32>>,
    }

    impl<F: Future> Future for Probe<F> {
        type Output = F::Output;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
            self.polls.set(self.polls.get() + 1);
            self.inner.as_mut().poll(cx)
        }
    }

    fn probe<F: Future>(fut: F) -> (Probe<F>, Rc<Cell<u32>>) {
        let polls = Rc::new(Cell::new(0));
        (Probe { inner: Box::pin(fut), polls: polls.clone() }, polls)
    }

    async fn pauses(count: u32, value: u32) -> u32 {
        for _ in 0..count {
            Yield(false).await;
        }
        value
    }

    #[test]
    fn both_futures_progress_in_every_poll() {
        let (a, a_polls) = probe(pauses(2, 1));
        let (b, b_polls) = probe(pauses(5, 2));
        let mut joined = pin!(join2(a, b));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(joined.as_mut().poll(&mut cx).is_pending());
        assert_eq!((a_polls.get(), b_polls.get()), (1, 1), "первый опрос должен коснуться обоих: join идёт одновременно, не по очереди");
    }

    #[test]
    fn finished_futures_are_not_polled_again() {
        let (a, a_polls) = probe(pauses(2, 7));
        let (b, b_polls) = probe(pauses(5, 8));
        let mut joined = pin!(join2(a, b));
        let mut cx = Context::from_waker(Waker::noop());
        let mut rounds = 0;
        let result = loop {
            rounds += 1;
            assert!(rounds < 20, "join не закончился");
            if let Poll::Ready(pair) = joined.as_mut().poll(&mut cx) {
                break pair;
            }
        };
        assert_eq!(result, (7, 8));
        assert_eq!(rounds, 6, "ждём самого медленного: пять пауз и один завершающий опрос");
        assert_eq!(a_polls.get(), 3, "быстрый future получает ровно три опроса: две паузы и готовность, дальше его трогать нельзя");
        assert_eq!(b_polls.get(), 6);
    }

    #[test]
    fn the_two_results_may_have_different_types() {
        let mut joined = pin!(join2(async { 1_u8 }, async { "x".to_string() }));
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(joined.as_mut().poll(&mut cx), Poll::Ready((1, "x".to_string())));
    }

    #[test]
    fn the_slower_first_future_does_not_block_the_second() {
        let (slow, slow_polls) = probe(pauses(3, 1));
        let (fast, fast_polls) = probe(pauses(0, 2));
        let mut joined = pin!(join2(slow, fast));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(joined.as_mut().poll(&mut cx).is_pending());
        assert_eq!(fast_polls.get(), 1, "второй future готов в первом же опросе");
        for _ in 0..3 {
            let _ = joined.as_mut().poll(&mut cx);
        }
        assert_eq!((slow_polls.get(), fast_polls.get()), (4, 1));
    }
}
