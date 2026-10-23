use mini_exec::{block_on, sleep, yield_now, Executor};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};

fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
    }
}

type Log = Rc<RefCell<Vec<String>>>;

fn note(log: &Log, text: &str) {
    log.borrow_mut().push(text.to_string());
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

fn counted<F: Future>(fut: F) -> (Counted<F>, Rc<Cell<u32>>) {
    let polls = Rc::new(Cell::new(0));
    (Counted { inner: Box::pin(fut), polls: polls.clone() }, polls)
}

struct WakeSelf {
    wakes_left: u32,
    extra: u32,
}

impl Future for WakeSelf {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.wakes_left == 0 {
            return Poll::Ready(());
        }
        self.wakes_left -= 1;
        for _ in 0..=self.extra {
            cx.waker().wake_by_ref();
        }
        Poll::Pending
    }
}

#[test]
fn block_on_returns_the_value() {
    assert_eq!(within(3, "block_on", || block_on(async { 20 + 22 })), 42);
    let text = within(3, "block_on", || {
        block_on(async {
            yield_now().await;
            yield_now().await;
            "done".to_string()
        })
    });
    assert_eq!(text, "done");
}

#[test]
fn block_on_sleeps_while_waiting() {
    let (polls, elapsed) = within(5, "block_on", || {
        let started = Instant::now();
        let (fut, polls) = counted(sleep(Duration::from_millis(80)));
        block_on(fut);
        (polls.get(), started.elapsed())
    });
    assert!(elapsed >= Duration::from_millis(80), "sleep вернулся раньше срока");
    assert!(polls <= 4, "за 80 мс сделано {polls} опросов: пока future ждёт, поток обязан спать");
}

#[test]
fn spawn_and_join() {
    let value = within(5, "run_until", || {
        let executor = Executor::new();
        let ex = executor.clone();
        executor.run_until(async move {
            let a = ex.spawn(async { 1 });
            let b = ex.spawn(async {
                yield_now().await;
                2
            });
            let c = ex.spawn(async { "three".len() });
            a.await + b.await + c.await as i32
        })
    });
    assert_eq!(value, 8);
}

#[test]
fn tasks_start_in_spawn_order_and_take_turns() {
    within(5, "run", || {
        let executor = Executor::new();
        let log: Log = Rc::default();
        for name in ["a", "b", "c"] {
            let log = log.clone();
            executor.spawn(async move {
                note(&log, &format!("{name}1"));
                yield_now().await;
                note(&log, &format!("{name}2"));
            });
        }
        executor.run();
        assert_eq!(*log.borrow(), vec!["a1", "b1", "c1", "a2", "b2", "c2"], "порядок опросов: по очереди, после уступки в хвост");
    });
}

#[test]
fn one_poll_per_wake_and_no_idle_polling() {
    within(5, "run", || {
        let executor = Executor::new();
        let (fut, polls) = counted(WakeSelf { wakes_left: 3, extra: 0 });
        executor.spawn(fut);
        executor.run();
        assert_eq!(polls.get(), 4, "три пробуждения дают три Pending и один финальный опрос");
    });
}

#[test]
fn several_wakes_before_a_poll_make_one_poll() {
    within(5, "run", || {
        let executor = Executor::new();
        let (fut, polls) = counted(WakeSelf { wakes_left: 2, extra: 4 });
        executor.spawn(fut);
        executor.run();
        assert_eq!(polls.get(), 3, "пять wake за один опрос должны дать одно повторное опрашивание, а не пять");
    });
}

#[test]
fn unwoken_tasks_are_never_polled_again() {
    within(5, "run_until", || {
        let executor = Executor::new();
        let polls = Rc::new(Cell::new(0));
        let mine = polls.clone();
        let _stuck = executor.spawn(std::future::poll_fn(move |_cx| {
            mine.set(mine.get() + 1);
            Poll::<()>::Pending
        }));
        executor.run_until(async {
            for _ in 0..5 {
                yield_now().await;
            }
        });
        assert_eq!(polls.get(), 1, "задача, которую никто не будил, получила лишние опросы");
        assert_eq!(executor.pending_tasks(), 1);
    });
}

#[test]
fn detached_tasks_still_finish() {
    within(5, "run", || {
        let executor = Executor::new();
        let done = Rc::new(Cell::new(0));
        for _ in 0..5 {
            let done = done.clone();
            drop(executor.spawn(async move {
                yield_now().await;
                done.set(done.get() + 1);
            }));
        }
        assert_eq!(executor.pending_tasks(), 5);
        executor.run();
        assert_eq!(done.get(), 5);
        assert_eq!(executor.pending_tasks(), 0);
    });
}

#[test]
fn run_without_tasks_returns_at_once() {
    within(2, "пустой run", || Executor::new().run());
}

#[test]
fn sleeps_wait_side_by_side() {
    let (order, elapsed) = within(5, "sleep", || {
        let executor = Executor::new();
        let log: Log = Rc::default();
        let started = Instant::now();
        for (name, ms) in [("slow", 90), ("fast", 30), ("middle", 60)] {
            let log = log.clone();
            executor.spawn(async move {
                sleep(Duration::from_millis(ms)).await;
                note(&log, name);
            });
        }
        executor.run();
        let order = log.borrow().clone();
        (order, started.elapsed())
    });
    assert_eq!(order, vec!["fast", "middle", "slow"], "задачи заканчиваются по порядку сроков");
    assert!(elapsed < Duration::from_millis(170), "сроки складываются ({elapsed:?}): таймеры должны ждать одновременно");
}

#[test]
fn a_task_can_spawn_and_join_another() {
    let total = within(5, "run_until", || {
        let executor = Executor::new();
        let ex = executor.clone();
        executor.run_until(async move {
            let inner = ex.clone();
            let outer = ex.spawn(async move {
                let child = inner.spawn(async {
                    yield_now().await;
                    10
                });
                child.await + 1
            });
            outer.await
        })
    });
    assert_eq!(total, 11);
}

#[test]
fn the_executor_sleeps_until_a_foreign_thread_wakes_it() {
    let (polls, elapsed) = within(5, "run", || {
        let executor = Executor::new();
        let polls = Rc::new(Cell::new(0));
        let mine = polls.clone();
        let started = Instant::now();
        let mut remote = false;
        executor.spawn(std::future::poll_fn(move |cx| {
            mine.set(mine.get() + 1);
            if remote {
                return Poll::Ready(());
            }
            remote = true;
            let waker = cx.waker().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(70));
                waker.wake();
            });
            Poll::Pending
        }));
        executor.run();
        (polls.get(), started.elapsed())
    });
    assert_eq!(polls, 2, "пока никто не будил, задачу опрашивать нельзя");
    assert!(elapsed >= Duration::from_millis(70));
}

#[test]
fn a_handle_of_a_finished_task_is_ready_at_once() {
    within(5, "block_on", || {
        let executor = Executor::new();
        let handle = executor.spawn(async { vec![1, 2, 3] });
        executor.run();
        assert_eq!(block_on(handle), vec![1, 2, 3]);
    });
}

#[test]
fn the_main_future_may_borrow_local_data() {
    within(5, "run_until", || {
        let executor = Executor::new();
        let data = [4, 5, 6];
        let counter = Rc::new(Cell::new(0));
        let mine = counter.clone();
        executor.spawn(async move {
            for _ in 0..3 {
                yield_now().await;
                mine.set(mine.get() + 1);
            }
        });
        let sum = executor.run_until(async {
            for _ in 0..4 {
                yield_now().await;
            }
            data.iter().sum::<i32>()
        });
        assert_eq!(sum, 15);
        assert_eq!(counter.get(), 3, "пока главный future ждал, остальные задачи шли");
        assert_eq!(executor.pending_tasks(), 0);
    });
}

#[test]
fn many_tasks_finish_without_losing_any() {
    let finished = within(10, "run", || {
        let executor = Executor::new();
        let total = Rc::new(Cell::new(0_u32));
        for i in 0..300_u32 {
            let total = total.clone();
            executor.spawn(async move {
                for _ in 0..(i % 5) {
                    yield_now().await;
                }
                total.set(total.get() + 1);
            });
        }
        executor.run();
        total.get()
    });
    assert_eq!(finished, 300);
}

#[test]
fn wakers_from_other_threads_are_thread_safe() {
    let hits = Arc::new(AtomicUsize::new(0));
    let mine = hits.clone();
    within(5, "block_on", move || {
        block_on(async move {
            let mut waker_for_thread = None::<Waker>;
            let mut started = false;
            std::future::poll_fn(|cx| {
                if started {
                    return Poll::Ready(());
                }
                started = true;
                waker_for_thread = Some(cx.waker().clone());
                let waker = waker_for_thread.take().unwrap();
                let hits = mine.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(20));
                    hits.fetch_add(1, Ordering::SeqCst);
                    waker.wake();
                });
                Poll::Pending
            })
            .await;
        })
    });
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}
