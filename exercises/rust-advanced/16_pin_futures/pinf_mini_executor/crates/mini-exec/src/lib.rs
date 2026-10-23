use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// Доводит future до конца в текущем потоке.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    todo!()
}

#[derive(Clone)]
pub struct Executor {}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Executor {
        todo!()
    }

    pub fn spawn<T: 'static>(&self, fut: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        todo!()
    }

    pub fn run(&self) {
        todo!()
    }

    pub fn run_until<F: Future>(&self, fut: F) -> F::Output {
        todo!()
    }

    pub fn pending_tasks(&self) -> usize {
        todo!()
    }
}

pub struct JoinHandle<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        todo!()
    }
}

pub struct Sleep {}

pub fn sleep(delay: Duration) -> Sleep {
    todo!()
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}

pub struct YieldNow {}

pub fn yield_now() -> YieldNow {
    todo!()
}

impl Future for YieldNow {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}
