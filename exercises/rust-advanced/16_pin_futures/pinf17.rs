// Тот же исполнитель, но написанный кем-то другим: слот хранит future без `Option` и опрашивает то, что
// достал из очереди. Работало, пока задачи будили по одному разу. Теперь одна из задач вызывает `wake`
// дважды за опрос (так делают библиотеки, которые будят «на всякий случай»), и исполнитель паникует:
// «`async fn` resumed after completion». Задача закончилась, а её вторая запись в очереди осталась,
// и исполнитель опросил уже завершённый future.
// TODO: Почини исполнитель, чтобы лишние пробуждения были безвредны. Публичный API менять нельзя.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Task {
    future: Mutex<BoxFuture>,
    queue: Sender<Arc<Task>>,
    live: Arc<AtomicUsize>,
}

pub struct Executor {
    receiver: Receiver<Arc<Task>>,
    sender: Sender<Arc<Task>>,
    live: Arc<AtomicUsize>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Executor {
        let (sender, receiver) = channel();
        Executor { receiver, sender, live: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let task = Arc::new(Task {
            future: Mutex::new(Box::pin(future)),
            queue: self.sender.clone(),
            live: Arc::clone(&self.live),
        });
        self.live.fetch_add(1, Ordering::SeqCst);
        self.sender.send(task).expect("очередь закрыта");
    }

    pub fn run(&self) {
        while self.live.load(Ordering::SeqCst) > 0 {
            let task = self.receiver.recv().expect("очередь закрыта");
            let mut future = task.future.lock().unwrap();
            let waker = Waker::from(Arc::clone(&task));
            let mut cx = Context::from_waker(&waker);
            if future.as_mut().poll(&mut cx).is_ready() {
                task.live.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let queue = self.queue.clone();
        let _ = queue.send(self);
    }
}

fn main() {
    let executor = Executor::new();
    executor.spawn(async {});
    executor.run();
}

#[cfg(test)]
mod tests {
    use super::*;
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
    type Log = Arc<Mutex<Vec<&'static str>>>;

    fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
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

    struct WakeTwice {
        polled: bool,
    }

    impl Future for WakeTwice {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.polled {
                Poll::Ready(())
            } else {
                self.polled = true;
                cx.waker().wake_by_ref();
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    #[test]
    fn extra_wakes_do_not_poll_a_finished_task() {
        let executor = Executor::new();
        let events = log();
        let first = events.clone();
        executor.spawn(async move {
            WakeTwice { polled: false }.await;
            first.lock().unwrap().push("double");
        });
        let second = events.clone();
        executor.spawn(async move {
            for _ in 0..4 {
                Yield(false).await;
            }
            second.lock().unwrap().push("steady");
        });
        within(5, "run", move || executor.run());
        let mut done = events.lock().unwrap().clone();
        done.sort_unstable();
        assert_eq!(done, vec!["double", "steady"], "обе задачи должны дойти до конца");
    }

    #[test]
    fn an_old_wake_after_the_end_is_harmless() {
        let executor = Executor::new();
        let events = log();
        let slot = Arc::new(Mutex::new(None::<Waker>));
        let (keep, mine) = (slot.clone(), events.clone());
        executor.spawn(async move {
            std::future::poll_fn(|cx| {
                *keep.lock().unwrap() = Some(cx.waker().clone());
                Poll::Ready(())
            })
            .await;
            mine.lock().unwrap().push("first done");
        });
        let (stale, theirs) = (slot.clone(), events.clone());
        executor.spawn(async move {
            Yield(false).await;
            if let Some(waker) = stale.lock().unwrap().take() {
                waker.wake();
            }
            for _ in 0..3 {
                Yield(false).await;
            }
            theirs.lock().unwrap().push("second done");
        });
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["first done", "second done"]);
    }

    #[test]
    fn ordinary_tasks_still_work() {
        let executor = Executor::new();
        let events = log();
        for name in ["x", "y"] {
            let events = events.clone();
            executor.spawn(async move {
                Yield(false).await;
                events.lock().unwrap().push(name);
            });
        }
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["x", "y"]);
    }
}
