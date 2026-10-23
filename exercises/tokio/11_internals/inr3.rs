// Своему однопоточному executor'у не хватает `JoinHandle`: `spawn` должен возвращать то, что можно
// `await`, и по этому `await` получать результат задачи. Ждущий должен просыпаться, когда
// результат появился, и не должен крутиться впустую. Если handle выбросили, сама задача всё равно
// должна дойти до конца: результат тогда просто никому не нужен.
// TODO: Допиши `Executor::spawn` и `Future` для `JoinHandle`.
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type Job = Pin<Box<dyn Future<Output = ()>>>;

struct IdWaker {
    id: usize,
    ready: Arc<Mutex<VecDeque<usize>>>,
}

impl Wake for IdWaker {
    fn wake(self: Arc<Self>) {
        self.ready.lock().unwrap().push_back(self.id);
    }
}

struct Executor {
    jobs: RefCell<Vec<Option<Job>>>,
    ready: Arc<Mutex<VecDeque<usize>>>,
}

struct Slot<T> {
    result: Option<T>,
    waiter: Option<Waker>,
}

struct JoinHandle<T> {
    slot: Rc<RefCell<Slot<T>>>,
}

impl Executor {
    fn new() -> Executor {
        Executor { jobs: RefCell::new(Vec::new()), ready: Arc::new(Mutex::new(VecDeque::new())) }
    }

    fn spawn_raw(&self, job: Job) {
        let mut jobs = self.jobs.borrow_mut();
        jobs.push(Some(job));
        self.ready.lock().unwrap().push_back(jobs.len() - 1);
    }

    fn run(&self) {
        loop {
            let Some(id) = self.ready.lock().unwrap().pop_front() else { break };
            let Some(mut job) = self.jobs.borrow_mut()[id].take() else { continue };
            let waker = Waker::from(Arc::new(IdWaker { id, ready: self.ready.clone() }));
            if job.as_mut().poll(&mut Context::from_waker(&waker)).is_pending() {
                self.jobs.borrow_mut()[id] = Some(job);
            }
        }
    }

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        let _ = future;
        todo!()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let _ = cx;
        todo!()
    }
}

fn main() {
    let ex = Executor::new();
    let handle = ex.spawn(async { 6 * 7 });
    ex.spawn_raw(Box::pin(async move { println!("{}", handle.await) }));
    ex.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct YieldNow(bool);

    impl Future for YieldNow {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                return Poll::Ready(());
            }
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    async fn yield_times(n: usize) {
        for _ in 0..n {
            YieldNow(false).await;
        }
    }

    #[test]
    fn the_result_reaches_the_waiter() {
        let ex = Executor::new();
        let out = Rc::new(Cell::new(0));
        let handle = ex.spawn(async { 21 * 2 });
        let o = out.clone();
        ex.spawn_raw(Box::pin(async move { o.set(handle.await) }));
        ex.run();
        assert_eq!(out.get(), 42, "ждущий должен получить результат задачи");
    }

    #[test]
    fn the_waiter_sleeps_until_the_result_is_there() {
        let ex = Executor::new();
        let polls = Rc::new(Cell::new(0));
        let out = Rc::new(Cell::new(0));
        let slow = ex.spawn(async {
            yield_times(5).await;
            99
        });
        let (p, o) = (polls.clone(), out.clone());
        ex.spawn_raw(Box::pin(async move {
            let mut handle = slow;
            let value = std::future::poll_fn(|cx| {
                p.set(p.get() + 1);
                Pin::new(&mut handle).poll(cx)
            })
            .await;
            o.set(value);
        }));
        ex.run();
        assert_eq!(out.get(), 99, "результат медленной задачи должен дойти до ждущего");
        assert_eq!(polls.get(), 2, "ждущего опрашивают дважды: пока пусто, и когда его разбудили; без пробуждений вхолостую");
    }

    #[test]
    fn a_dropped_handle_does_not_cancel_the_task() {
        let ex = Executor::new();
        let done = Rc::new(Cell::new(false));
        let d = done.clone();
        let handle = ex.spawn(async move {
            yield_times(2).await;
            d.set(true);
        });
        drop(handle);
        ex.run();
        assert!(done.get(), "если handle выбросили, задача всё равно обязана дойти до конца");
    }

    #[test]
    fn a_handle_awaited_after_completion_returns_at_once() {
        let ex = Executor::new();
        let early = ex.spawn(async { "готово" });
        ex.run();
        let out = Rc::new(RefCell::new(String::new()));
        let o = out.clone();
        ex.spawn_raw(Box::pin(async move { *o.borrow_mut() = early.await.to_string() }));
        ex.run();
        assert_eq!(*out.borrow(), "готово", "задача кончилась до того, как на неё стали ждать: результат всё равно ждёт в слоте");
    }

    #[test]
    fn handles_chain_through_tasks() {
        let ex = Rc::new(Executor::new());
        let out = Rc::new(Cell::new(0));
        let inner = {
            let ex2 = ex.clone();
            ex.spawn(async move {
                let a = ex2.spawn(async { yield_times(1).await; 1 });
                let b = ex2.spawn(async { yield_times(3).await; 2 });
                a.await + b.await
            })
        };
        let o = out.clone();
        ex.spawn_raw(Box::pin(async move { o.set(inner.await * 10) }));
        ex.run();
        assert_eq!(out.get(), 30, "задача, породившая другие задачи и дождавшаяся их, тоже отдаёт результат");
    }
}
