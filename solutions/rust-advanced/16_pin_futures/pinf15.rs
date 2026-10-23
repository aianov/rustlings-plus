use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};

struct Parker {
    woken: Mutex<bool>,
    changed: Condvar,
}

impl Wake for Parker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        *self.woken.lock().unwrap() = true;
        self.changed.notify_one();
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let parker = Arc::new(Parker { woken: Mutex::new(false), changed: Condvar::new() });
    let waker = Waker::from(Arc::clone(&parker));
    let mut cx = Context::from_waker(&waker);
    loop {
        *parker.woken.lock().unwrap() = false;
        if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
            return value;
        }
        let mut woken = parker.woken.lock().unwrap();
        while !*woken {
            woken = parker.changed.wait(woken).unwrap();
        }
    }
}

fn main() {
    println!("{}", block_on(async { 2 + 2 }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::thread;
    use std::time::Duration;

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

    struct ThinkThenPend {
        stage: u32,
    }

    impl Future for ThinkThenPend {
        type Output = &'static str;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
            self.stage += 1;
            if self.stage == 1 {
                let waker = cx.waker().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(5));
                    waker.wake();
                });
                thread::sleep(Duration::from_millis(60));
                Poll::Pending
            } else {
                Poll::Ready("done")
            }
        }
    }

    #[test]
    fn plain_futures_still_work() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    #[test]
    fn a_future_that_wakes_itself_during_poll_does_not_hang() {
        let value = within(3, "block_on с самопробуждением", || {
            block_on(async {
                for _ in 0..3 {
                    Yield(false).await;
                }
                "ok"
            })
        });
        assert_eq!(value, "ok");
    }

    #[test]
    fn a_wake_that_arrives_while_the_future_is_still_thinking_is_kept() {
        let value = within(3, "block_on с пробуждением во время poll", || block_on(ThinkThenPend { stage: 0 }));
        assert_eq!(value, "done", "пробуждение пришло, пока шёл poll, и его нельзя выбрасывать после");
    }

    #[test]
    fn waiting_for_another_thread_still_works() {
        let value = within(3, "block_on с пробуждением из другого потока", || {
            block_on(async {
                let waker_slot = Arc::new(Mutex::new(None::<Waker>));
                let remote = Arc::clone(&waker_slot);
                let done = Arc::new(Mutex::new(false));
                let flag = Arc::clone(&done);
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(30));
                    *flag.lock().unwrap() = true;
                    if let Some(waker) = remote.lock().unwrap().take() {
                        waker.wake();
                    }
                });
                std::future::poll_fn(move |cx| {
                    let mut flag = done.lock().unwrap();
                    if *flag {
                        Poll::Ready(5)
                    } else {
                        *waker_slot.lock().unwrap() = Some(cx.waker().clone());
                        let _ = &mut flag;
                        Poll::Pending
                    }
                })
                .await
            })
        });
        assert_eq!(value, 5);
    }
}
