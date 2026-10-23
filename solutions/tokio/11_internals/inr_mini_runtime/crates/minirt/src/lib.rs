use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

const MAIN: usize = usize::MAX;

type Job = Pin<Box<dyn Future<Output = ()>>>;

struct Shared {
    ready: Mutex<VecDeque<usize>>,
    owner: Thread,
}

struct IdWaker {
    id: usize,
    shared: Arc<Shared>,
}

impl Wake for IdWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.shared.ready.lock().unwrap().push_back(self.id);
        self.shared.owner.unpark();
    }
}

enum Clock {
    Real(Instant),
    Paused(Cell<Duration>),
}

#[derive(Default)]
struct Timers {
    heap: BinaryHeap<Reverse<(Duration, u64)>>,
    live: HashMap<u64, Waker>,
    next_id: u64,
}

impl Timers {
    fn register(&mut self, deadline: Duration, waker: Waker) -> u64 {
        self.next_id += 1;
        self.heap.push(Reverse((deadline, self.next_id)));
        self.live.insert(self.next_id, waker);
        self.next_id
    }

    fn next_deadline(&mut self) -> Option<Duration> {
        while let Some(&Reverse((deadline, id))) = self.heap.peek() {
            if self.live.contains_key(&id) {
                return Some(deadline);
            }
            self.heap.pop();
        }
        None
    }

    fn fire_due(&mut self, now: Duration) {
        while let Some(&Reverse((deadline, id))) = self.heap.peek() {
            if deadline > now {
                break;
            }
            self.heap.pop();
            if let Some(waker) = self.live.remove(&id) {
                waker.wake();
            }
        }
    }
}

struct Inner {
    shared: Arc<Shared>,
    jobs: RefCell<Vec<Option<Job>>>,
    timers: RefCell<Timers>,
    clock: Clock,
}

impl Inner {
    fn now(&self) -> Duration {
        match &self.clock {
            Clock::Real(start) => start.elapsed(),
            Clock::Paused(now) => now.get(),
        }
    }

    fn waker(&self, id: usize) -> Waker {
        Waker::from(Arc::new(IdWaker { id, shared: self.shared.clone() }))
    }

    fn spawn_raw(&self, job: Job) {
        let mut jobs = self.jobs.borrow_mut();
        jobs.push(Some(job));
        let id = jobs.len() - 1;
        drop(jobs);
        self.shared.ready.lock().unwrap().push_back(id);
    }

    fn poll_job(&self, id: usize) {
        let Some(mut job) = self.jobs.borrow_mut().get_mut(id).and_then(Option::take) else {
            return;
        };
        let waker = self.waker(id);
        if job.as_mut().poll(&mut Context::from_waker(&waker)).is_pending() {
            self.jobs.borrow_mut()[id] = Some(job);
        }
    }

    fn idle(&self) {
        let now = self.now();
        let next = self.timers.borrow_mut().next_deadline();
        match (&self.clock, next) {
            (Clock::Paused(clock), Some(deadline)) => {
                if deadline > now {
                    clock.set(deadline);
                }
            }
            (Clock::Real(_), Some(deadline)) => {
                if deadline > now {
                    thread::park_timeout(deadline - now);
                }
            }
            (_, None) => thread::park(),
        }
        let now = self.now();
        self.timers.borrow_mut().fire_due(now);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Inner>>> = const { RefCell::new(None) };
}

struct EnterGuard(Option<Rc<Inner>>);

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|current| *current.borrow_mut() = self.0.take());
    }
}

fn current() -> Rc<Inner> {
    CURRENT.with(|current| current.borrow().clone()).expect("вызов вне рантайма: сначала block_on")
}

pub struct Runtime {
    inner: Rc<Inner>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

impl Runtime {
    fn with_clock(clock: Clock) -> Runtime {
        let shared = Arc::new(Shared { ready: Mutex::new(VecDeque::new()), owner: thread::current() });
        Runtime { inner: Rc::new(Inner { shared, jobs: RefCell::new(Vec::new()), timers: RefCell::new(Timers::default()), clock }) }
    }

    pub fn new() -> Runtime {
        Runtime::with_clock(Clock::Real(Instant::now()))
    }

    pub fn paused() -> Runtime {
        Runtime::with_clock(Clock::Paused(Cell::new(Duration::ZERO)))
    }

    pub fn elapsed(&self) -> Duration {
        self.inner.now()
    }

    pub fn live_timers(&self) -> usize {
        self.inner.timers.borrow().live.len()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        spawn_on(&self.inner, future)
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _guard = EnterGuard(CURRENT.with(|current| current.borrow_mut().replace(self.inner.clone())));
        let mut future = pin!(future);
        let main_waker = self.inner.waker(MAIN);
        self.inner.shared.ready.lock().unwrap().push_back(MAIN);
        loop {
            loop {
                let next = self.inner.shared.ready.lock().unwrap().pop_front();
                let Some(id) = next else { break };
                if id == MAIN {
                    if let Poll::Ready(value) = future.as_mut().poll(&mut Context::from_waker(&main_waker)) {
                        return value;
                    }
                } else {
                    self.inner.poll_job(id);
                }
            }
            self.inner.idle();
        }
    }
}

fn spawn_on<F>(inner: &Inner, future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let slot = Rc::new(RefCell::new(Slot { result: None, waiter: None }));
    let theirs = slot.clone();
    inner.spawn_raw(Box::pin(async move {
        let value = future.await;
        let mut slot = theirs.borrow_mut();
        slot.result = Some(value);
        if let Some(waiter) = slot.waiter.take() {
            waiter.wake();
        }
    }));
    JoinHandle { slot }
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    spawn_on(&current(), future)
}

pub fn now() -> Duration {
    current().now()
}

struct Slot<T> {
    result: Option<T>,
    waiter: Option<Waker>,
}

pub struct JoinHandle<T> {
    slot: Rc<RefCell<Slot<T>>>,
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.slot.borrow_mut();
        match slot.result.take() {
            Some(value) => Poll::Ready(value),
            None => {
                slot.waiter = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

pub struct Sleep {
    inner: Rc<Inner>,
    deadline: Duration,
    id: Option<u64>,
}

pub fn sleep(duration: Duration) -> Sleep {
    let inner = current();
    let deadline = inner.now() + duration;
    Sleep { inner, deadline, id: None }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.inner.now() >= self.deadline {
            return Poll::Ready(());
        }
        let inner = self.inner.clone();
        let mut timers = inner.timers.borrow_mut();
        match self.id.and_then(|id| timers.live.get_mut(&id)) {
            Some(slot) => *slot = cx.waker().clone(),
            None => self.id = Some(timers.register(self.deadline, cx.waker().clone())),
        }
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.inner.timers.borrow_mut().live.remove(&id);
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
            return Poll::Ready(());
        }
        self.yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
