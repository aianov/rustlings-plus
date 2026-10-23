// Пора собрать всё в простой исполнитель. Он принимает задачи (`spawn`), хранит каждую в `Arc<Task>`
// вместе с её future, а очередью служит канал: `Waker` задачи при `wake` кладёт её обратно в очередь.
// `run` берёт задачи из очереди и опрашивает, пока не закончатся все; если очередь пуста, а задачи ещё
// есть, поток спит в `recv` (опрашивать вхолостую нельзя). Задача, которая вернула `Pending`, возвращается
// в слот и ждёт, пока её разбудят.
// Заготовка: типы и конструкторы готовы, нужны три места.
// TODO: Допиши `Spawner::spawn`, `Wake for Task` и `Executor::run`.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Task {
    future: Mutex<Option<BoxFuture>>,
    queue: Sender<Arc<Task>>,
    live: Arc<AtomicUsize>,
}

pub struct Executor {
    receiver: Receiver<Arc<Task>>,
    sender: Sender<Arc<Task>>,
    live: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub struct Spawner {
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

    pub fn spawner(&self) -> Spawner {
        Spawner { sender: self.sender.clone(), live: Arc::clone(&self.live) }
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.spawner().spawn(future);
    }

    /// Опрашивает задачи, пока не закончатся все.
    pub fn run(&self) {
        todo!("достать задачу из очереди, опросить, вернуть в слот при Pending")
    }
}

impl Spawner {
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        todo!("завести Task, посчитать её живой, поставить в очередь")
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        todo!("поставить задачу обратно в очередь")
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

    #[test]
    fn every_task_runs_to_the_end() {
        let executor = Executor::new();
        let events = log();
        for name in ["a", "b", "c"] {
            let events = events.clone();
            executor.spawn(async move {
                Yield(false).await;
                events.lock().unwrap().push(name);
            });
        }
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn tasks_take_turns_at_every_await() {
        let executor = Executor::new();
        let events = log();
        for (first, second) in [("a1", "a2"), ("b1", "b2")] {
            let events = events.clone();
            executor.spawn(async move {
                events.lock().unwrap().push(first);
                Yield(false).await;
                events.lock().unwrap().push(second);
            });
        }
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["a1", "b1", "a2", "b2"], "после Pending задача уходит в конец очереди, и очередь справедлива");
    }

    #[test]
    fn a_task_can_spawn_another_one() {
        let executor = Executor::new();
        let spawner = executor.spawner();
        let events = log();
        let outer = events.clone();
        executor.spawn(async move {
            outer.lock().unwrap().push("outer");
            let inner = outer.clone();
            spawner.spawn(async move {
                inner.lock().unwrap().push("inner");
            });
            Yield(false).await;
            outer.lock().unwrap().push("outer done");
        });
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["outer", "inner", "outer done"]);
    }

    #[test]
    fn a_task_woken_from_another_thread_is_polled_again() {
        let executor = Executor::new();
        let events = log();
        let polls = Arc::new(AtomicUsize::new(0));
        let (seen, mine) = (events.clone(), polls.clone());
        executor.spawn(async move {
            let mut remote = None;
            std::future::poll_fn(|cx| {
                mine.fetch_add(1, Ordering::SeqCst);
                match &remote {
                    None => {
                        let waker = cx.waker().clone();
                        remote = Some(thread::spawn(move || {
                            thread::sleep(Duration::from_millis(60));
                            waker.wake();
                        }));
                        Poll::Pending
                    }
                    Some(_) => Poll::Ready(()),
                }
            })
            .await;
            seen.lock().unwrap().push("woken");
        });
        within(5, "run", move || executor.run());
        assert_eq!(*events.lock().unwrap(), vec!["woken"]);
        assert_eq!(polls.load(Ordering::SeqCst), 2, "пока никто не разбудил, задачу опрашивать нельзя: поток спит в очереди");
    }

    #[test]
    fn run_returns_at_once_when_there_is_nothing_to_do() {
        let executor = Executor::new();
        within(2, "пустой run", move || executor.run());
    }
}
