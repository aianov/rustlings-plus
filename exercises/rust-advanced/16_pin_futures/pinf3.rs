// Любая `async fn` при вызове ничего не делает: она возвращает автомат, и первый шаг тела выполняется
// только на первом `poll`. Две задачи на это.
// Первая: `start_later` должна возвращать future, который при создании ничего не пишет в журнал,
// а при первом опросе пишет `"start"` и отдаёт новую длину журнала.
// Вторая: `greet_both` это `async fn` с двумя `.await`. Компилятор разворачивает такую функцию в enum
// состояний. Сделай то же самое руками: `GreetBothManual` должен опрашиваться ровно так же, как `greet_both`
// (тест сравнивает число опросов и результат на разных задержках).
// TODO: Допиши `start_later` и `poll` для `GreetBothManual`.
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

pub type Log = Rc<RefCell<Vec<&'static str>>>;

pub struct Delay {
    left: u32,
    label: &'static str,
    done: bool,
}

impl Delay {
    pub fn new(left: u32, label: &'static str) -> Delay {
        Delay { left, label, done: false }
    }
}

impl Future for Delay {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        assert!(!self.done, "Delay `{}` опрошен после Ready", self.label);
        if self.left == 0 {
            self.done = true;
            Poll::Ready(self.label)
        } else {
            self.left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub async fn greet_both(first: Delay, second: Delay) -> String {
    let a = first.await;
    let b = second.await;
    format!("{a}+{b}")
}

pub enum GreetBothManual {
    First { first: Delay, second: Delay },
    Second { a: &'static str, second: Delay },
    Done,
}

impl GreetBothManual {
    pub fn new(first: Delay, second: Delay) -> GreetBothManual {
        GreetBothManual::First { first, second }
    }
}

#[allow(clippy::manual_async_fn)]
pub fn start_later(log: Log) -> impl Future<Output = usize> {
    todo!("ничего не делать, пока future не опросили");
    std::future::ready(0)
}

impl Future for GreetBothManual {
    type Output = String;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<String> {
        todo!("два состояния и Done")
    }
}

fn main() {
    let log: Log = Rc::default();
    let _later = start_later(log);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;
    use std::task::Waker;

    fn polls_to_finish<T>(fut: impl Future<Output = T>) -> (T, u32) {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        for n in 1..=100 {
            if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
                return (value, n);
            }
        }
        panic!("future не завершился за 100 опросов");
    }

    #[test]
    fn manual_machine_matches_the_async_fn() {
        for (d1, d2) in [(0, 0), (0, 3), (2, 0), (3, 2), (1, 1)] {
            let by_hand = polls_to_finish(GreetBothManual::new(Delay::new(d1, "x"), Delay::new(d2, "y")));
            let compiled = polls_to_finish(greet_both(Delay::new(d1, "x"), Delay::new(d2, "y")));
            assert_eq!(by_hand, compiled, "задержки {d1} и {d2}: рукописный автомат ведёт себя иначе, чем async fn");
            assert_eq!(by_hand, ("x+y".to_string(), d1 + d2 + 1));
        }
    }

    #[test]
    fn the_second_delay_starts_only_after_the_first() {
        let mut machine = pin!(GreetBothManual::new(Delay::new(1, "a"), Delay::new(0, "b")));
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(machine.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(machine.as_mut().poll(&mut cx), Poll::Ready("a+b".to_string()), "второй Delay готов сразу после первого, в том же опросе");
    }

    #[test]
    fn creating_the_future_does_nothing() {
        let log: Log = Rc::default();
        let later = start_later(log.clone());
        assert!(log.borrow().is_empty(), "пока future не опрошен, в журнале пусто");
        let (len, polls) = polls_to_finish(later);
        assert_eq!((len, polls), (1, 1));
        assert_eq!(*log.borrow(), vec!["start"]);
    }

    #[test]
    fn order_of_polling_decides_the_order_of_effects() {
        let log: Log = Rc::default();
        let first = start_later(log.clone());
        let second = start_later(log.clone());
        assert_eq!(polls_to_finish(second).0, 1, "второй future опрошен первым и увидел журнал из одной записи");
        assert_eq!(polls_to_finish(first).0, 2);
    }
}
