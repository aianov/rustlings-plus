// Комбинатор `map`: берёт future и функцию и возвращает новый future, который отдаёт результат
// старого, пропущенный через функцию. Как `Iterator::map`, только для ожидания. Для простоты работаем
// с future, которые можно двигать (`Unpin`), и с функциями, которые тоже `Unpin`: тогда внутренний future
// закрепляется на лету через `Pin::new(&mut ..)`.
// Функция вызывается ровно один раз и только когда внутренний future готов. Опрос после `Ready`
// должен паниковать.
// TODO: Допиши `poll` для `Map`.
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Map<F, G> {
    inner: F,
    f: Option<G>,
}

pub fn map<F, G, T>(inner: F, f: G) -> Map<F, G>
where
    F: Future + Unpin,
    G: FnOnce(F::Output) -> T,
{
    Map { inner, f: Some(f) }
}

impl<F, G, T> Future for Map<F, G>
where
    F: Future + Unpin,
    G: FnOnce(F::Output) -> T + Unpin,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        todo!("опросить inner; если готов, применить f ровно один раз")
    }
}

fn main() {
    let _mapped = map(std::future::ready(1), |n| n + 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::task::Waker;

    struct Slow {
        left: u32,
        done: bool,
    }

    impl Future for Slow {
        type Output = u32;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
            assert!(!self.done, "Slow опрошен после Ready");
            if self.left == 0 {
                self.done = true;
                Poll::Ready(10)
            } else {
                self.left -= 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn slow(left: u32) -> Slow {
        Slow { left, done: false }
    }

    fn poll<F: Future + Unpin>(fut: &mut F) -> Poll<F::Output> {
        Pin::new(fut).poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn the_result_goes_through_the_function() {
        let mut fut = map(slow(2), |n| n * 3);
        assert_eq!(poll(&mut fut), Poll::Pending);
        assert_eq!(poll(&mut fut), Poll::Pending);
        assert_eq!(poll(&mut fut), Poll::Ready(30));
    }

    #[test]
    fn the_function_runs_once_and_only_when_ready() {
        let calls = Cell::new(0);
        let mut fut = map(slow(3), |n| {
            calls.set(calls.get() + 1);
            n
        });
        for _ in 0..3 {
            assert_eq!(poll(&mut fut), Poll::Pending);
            assert_eq!(calls.get(), 0, "функцию нельзя звать, пока внутренний future не готов");
        }
        assert_eq!(poll(&mut fut), Poll::Ready(10));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn maps_can_be_stacked() {
        let mut fut = map(map(slow(1), |n| n + 1), |n| format!("<{n}>"));
        assert_eq!(poll(&mut fut), Poll::Pending);
        assert_eq!(poll(&mut fut), Poll::Ready("<11>".to_string()), "результат одной обёртки идёт во вторую");
    }

    #[test]
    fn works_on_ready_futures_and_changes_the_type() {
        let mut fut = map(std::future::ready("42"), |text| text.parse::<u8>().unwrap());
        assert_eq!(poll(&mut fut), Poll::Ready(42));
    }

    #[test]
    fn polling_after_ready_is_a_loud_error() {
        let mut fut = map(slow(0), |n| n);
        assert_eq!(poll(&mut fut), Poll::Ready(10));
        let again = catch_unwind(AssertUnwindSafe(|| poll(&mut fut)));
        assert!(again.is_err(), "после Ready map должен паниковать, а не вызывать функцию второй раз");
    }
}
