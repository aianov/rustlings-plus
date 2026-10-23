use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

pub struct Map<F: Future, G> {
    inner: Pin<Box<F>>,
    f: Option<G>,
}

pub fn map<F, G, T>(inner: F, f: G) -> Map<F, G>
where
    F: Future,
    G: FnOnce(F::Output) -> T,
{
    Map { inner: Box::pin(inner), f: Some(f) }
}

impl<F, G, T> Future for Map<F, G>
where
    F: Future,
    G: FnOnce(F::Output) -> T + Unpin,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.get_mut();
        match this.inner.as_mut().poll(cx) {
            Poll::Ready(value) => {
                let f = this.f.take().expect("Map опрошен после Ready");
                Poll::Ready(f(value))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub fn poll_twice<F: Future>(mut fut: Pin<&mut F>) -> (Poll<F::Output>, Poll<F::Output>) {
    let mut cx = Context::from_waker(Waker::noop());
    let first = fut.as_mut().poll(&mut cx);
    let second = fut.as_mut().poll(&mut cx);
    (first, second)
}

fn main() {
    let _mapped = map(std::future::ready(1), |n| n + 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;

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

    fn finish<F: Future>(fut: F) -> (F::Output, u32) {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        for n in 1..=50 {
            if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
                return (value, n);
            }
        }
        panic!("future не завершился");
    }

    #[test]
    fn map_over_an_async_block() {
        let fut = map(
            async {
                Yield(false).await;
                20
            },
            |n| n + 1,
        );
        assert_eq!(finish(fut), (21, 2));
    }

    #[test]
    fn map_over_a_block_that_borrows_its_own_locals() {
        let fut = map(
            async {
                let words = ["a", "b", "c"];
                let longest = words.iter().min_by_key(|w| w.len()).unwrap();
                Yield(false).await;
                *longest
            },
            |w| w.to_uppercase(),
        );
        assert_eq!(finish(fut).0, "A");
    }

    #[test]
    fn maps_of_maps_still_work() {
        let fut = map(map(async { 1 }, |n| n + 1), |n| n * 10);
        assert_eq!(finish(fut), (20, 1));
    }

    #[test]
    fn the_function_runs_only_when_ready() {
        let calls = std::cell::Cell::new(0);
        let fut = map(
            async {
                Yield(false).await;
                Yield(false).await;
            },
            |()| calls.set(calls.get() + 1),
        );
        let ((), polls) = finish(fut);
        assert_eq!((polls, calls.get()), (3, 1));
    }

    #[test]
    fn poll_twice_works_on_pinned_async_blocks() {
        let pair = poll_twice(pin!(async {
            Yield(false).await;
            5
        }));
        assert_eq!(pair, (Poll::Pending, Poll::Ready(5)));
    }
}
