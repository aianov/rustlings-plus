// Нужен `block_on`: функция, которая берёт future, доводит его до конца и возвращает результат, блокируя
// вызывающий поток. На ней держатся `main` с асинхронным кодом, тесты и маленькие утилиты.
// Крутить `poll` без остановки нельзя: поток будет жечь процессор, пока future ждёт ответа от сети.
// Нужно усыпить поток до тех пор, пока future не скажет, что его пора опросить снова. «Скажет» это
// `Waker`: его будит другой поток (таймер, сокет) или сам future прямо во время `poll`.
// Простой способ: `Waker` хранит ссылку на поток (`Thread`), `wake` делает `unpark`, а цикл после `Pending`
// делает `park`.
// TODO: Допиши `wake` и `block_on`. Тест считает опросы: на ожидании в 60 мс их должно быть единицы.
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        todo!("разбудить поток")
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    todo!("закрепить future, получить Waker, цикл poll и park")
}

fn main() {
    println!("{}", block_on(async { 2 + 2 }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
        }
    }
    #[derive(Default)]
    struct TimerState {
        fired: bool,
        waker: Option<Waker>,
    }

    struct Timer(Arc<Mutex<TimerState>>);

    fn timer(ms: u64) -> Timer {
        let state = Arc::new(Mutex::new(TimerState::default()));
        let shared = Arc::clone(&state);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            let waker = {
                let mut state = shared.lock().unwrap();
                state.fired = true;
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        });
        Timer(state)
    }

    impl Future for Timer {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            let mut state = self.0.lock().unwrap();
            if state.fired {
                Poll::Ready(())
            } else {
                state.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

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

    struct Counted<F> {
        inner: Pin<Box<F>>,
        polls: Rc<Cell<u32>>,
    }

    impl<F: Future> Future for Counted<F> {
        type Output = F::Output;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
            self.polls.set(self.polls.get() + 1);
            self.inner.as_mut().poll(cx)
        }
    }

    #[test]
    fn ready_futures_return_at_once() {
        assert_eq!(block_on(async { 5 }), 5);
        assert_eq!(block_on(std::future::ready("x".to_string())), "x");
    }

    #[test]
    fn a_self_waking_future_does_not_get_stuck() {
        let value = within(3, "block_on", || {
            block_on(async {
                for _ in 0..5 {
                    Yield(false).await;
                }
                9
            })
        });
        assert_eq!(value, 9);
    }

    #[test]
    fn waits_for_another_thread_without_spinning() {
        let started = Instant::now();
        let polls = within(5, "block_on", || {
            let counted = Counted { inner: Box::pin(timer(60)), polls: Rc::new(Cell::new(0)) };
            let polls = counted.polls.clone();
            block_on(counted);
            polls.get()
        });
        assert!(started.elapsed() >= Duration::from_millis(60), "block_on вернулся раньше, чем сработал таймер");
        assert!(polls <= 6, "за 60 мс ожидания сделано {polls} опросов: поток должен спать в park, а не крутить цикл");
    }

    #[test]
    fn several_block_ons_run_side_by_side() {
        let started = Instant::now();
        let handles: Vec<_> = (0..4)
            .map(|i| {
                thread::spawn(move || {
                    block_on(async move {
                        timer(50).await;
                        timer(50).await;
                        i * 10
                    })
                })
            })
            .collect();
        let results = within(5, "несколько block_on", move || handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>());
        assert_eq!(results, vec![0, 10, 20, 30]);
        assert!(started.elapsed() < Duration::from_millis(900), "у каждого потока свой Waker, ждать друг друга они не должны");
    }

    #[test]
    fn the_output_may_be_any_type() {
        let text = block_on(async {
            timer(10).await;
            vec!["a".to_string(), "b".to_string()]
        });
        assert_eq!(text, vec!["a", "b"]);
    }
}
