use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

pub struct Runtime {}

pub struct JoinHandle<T> {
    marker: PhantomData<T>,
}

pub struct Sleep {}

pub struct YieldNow {}

impl Runtime {
    pub fn new() -> Runtime {
        todo!()
    }

    pub fn paused() -> Runtime {
        todo!()
    }

    pub fn elapsed(&self) -> Duration {
        todo!()
    }

    pub fn live_timers(&self) -> usize {
        todo!()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        todo!()
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        todo!()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    todo!()
}

pub fn now() -> Duration {
    todo!()
}

pub fn sleep(duration: Duration) -> Sleep {
    todo!()
}

pub fn yield_now() -> YieldNow {
    todo!()
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        todo!()
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}

impl Future for YieldNow {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}
