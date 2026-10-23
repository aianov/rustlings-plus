use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::Duration;

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
            return value;
        }
        thread::park();
    }
}

const MAIN: usize = 0;

#[derive(Default)]
struct Queue {
    ids: Mutex<VecDeque<usize>>,
    ready: Condvar,
}

impl Queue {
    fn push(&self, id: usize) {
        let mut ids = self.ids.lock().unwrap();
        if !ids.contains(&id) {
            ids.push_back(id);
            self.ready.notify_one();
        }
    }

    fn pop(&self) -> usize {
        let mut ids = self.ids.lock().unwrap();
        loop {
            if let Some(id) = ids.pop_front() {
                return id;
            }
            ids = self.ready.wait(ids).unwrap();
        }
    }
}

struct IdWaker {
    id: usize,
    queue: Arc<Queue>,
}

impl Wake for IdWaker {
    fn wake(self: Arc<Self>) {
        self.queue.push(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.push(self.id);
    }
}

type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;

struct Inner {
    queue: Arc<Queue>,
    tasks: RefCell<HashMap<usize, LocalFuture>>,
    next_id: Cell<usize>,
    live: Cell<usize>,
}

impl Inner {
    fn waker(&self, id: usize) -> Waker {
        Waker::from(Arc::new(IdWaker { id, queue: Arc::clone(&self.queue) }))
    }
}

#[derive(Clone)]
pub struct Executor {
    inner: Rc<Inner>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

struct JoinSlot<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

impl Executor {
    pub fn new() -> Executor {
        Executor {
            inner: Rc::new(Inner {
                queue: Arc::new(Queue::default()),
                tasks: RefCell::new(HashMap::new()),
                next_id: Cell::new(1),
                live: Cell::new(0),
            }),
        }
    }

    pub fn spawn<T: 'static>(&self, fut: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        let slot = Rc::new(RefCell::new(JoinSlot { value: None, waker: None }));
        let producer = Rc::clone(&slot);
        let task = async move {
            let value = fut.await;
            let mut slot = producer.borrow_mut();
            slot.value = Some(value);
            if let Some(waker) = slot.waker.take() {
                waker.wake();
            }
        };
        let id = self.inner.next_id.get();
        self.inner.next_id.set(id + 1);
        self.inner.tasks.borrow_mut().insert(id, Box::pin(task));
        self.inner.live.set(self.inner.live.get() + 1);
        self.inner.queue.push(id);
        JoinHandle { slot }
    }

    fn poll_task(&self, id: usize) {
        let Some(mut task) = self.inner.tasks.borrow_mut().remove(&id) else {
            return;
        };
        let waker = self.inner.waker(id);
        let mut cx = Context::from_waker(&waker);
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(()) => self.inner.live.set(self.inner.live.get() - 1),
            Poll::Pending => {
                self.inner.tasks.borrow_mut().insert(id, task);
            }
        }
    }

    pub fn run(&self) {
        while self.inner.live.get() > 0 {
            let id = self.inner.queue.pop();
            if id != MAIN {
                self.poll_task(id);
            }
        }
    }

    pub fn run_until<F: Future>(&self, fut: F) -> F::Output {
        let mut fut = pin!(fut);
        self.inner.queue.push(MAIN);
        loop {
            let id = self.inner.queue.pop();
            if id == MAIN {
                let waker = self.inner.waker(MAIN);
                let mut cx = Context::from_waker(&waker);
                if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
                    return value;
                }
            } else {
                self.poll_task(id);
            }
        }
    }

    pub fn pending_tasks(&self) -> usize {
        self.inner.live.get()
    }
}

pub struct JoinHandle<T> {
    slot: Rc<RefCell<JoinSlot<T>>>,
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.slot.borrow_mut();
        match slot.value.take() {
            Some(value) => Poll::Ready(value),
            None => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[derive(Default)]
struct SleepState {
    fired: bool,
    waker: Option<Waker>,
}

pub struct Sleep {
    state: Arc<Mutex<SleepState>>,
}

pub fn sleep(delay: Duration) -> Sleep {
    let state = Arc::new(Mutex::new(SleepState::default()));
    let shared = Arc::clone(&state);
    thread::spawn(move || {
        thread::sleep(delay);
        let waker = {
            let mut state = shared.lock().unwrap();
            state.fired = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    });
    Sleep { state }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.state.lock().unwrap();
        if state.fired {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct YieldNow {
    yielded: bool,
}

pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
