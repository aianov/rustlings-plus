use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct Trickle<R> {
    inner: R,
    step: usize,
    ready: bool,
}

impl<R> Trickle<R> {
    pub fn new(inner: R, step: usize) -> Trickle<R> {
        Trickle {
            inner,
            step: step.max(1),
            ready: false,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Trickle<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        if !this.ready {
            this.ready = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        this.ready = false;
        let step = this.step.min(buf.remaining());
        let read = {
            let mut part = ReadBuf::new(buf.initialize_unfilled_to(step));
            let poll = Pin::new(&mut this.inner).poll_read(cx, &mut part);
            poll.map_ok(|()| part.filled().len())
        };
        read.map_ok(|count| buf.advance(count))
    }
}

pub struct Chunked<W> {
    inner: W,
    step: usize,
}

impl<W> Chunked<W> {
    pub fn new(inner: W, step: usize) -> Chunked<W> {
        Chunked {
            inner,
            step: step.max(1),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Chunked<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        let part = &buf[..buf.len().min(this.step)];
        Pin::new(&mut this.inner).poll_write(cx, part)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[derive(Default)]
struct Seen {
    data: Vec<u8>,
    writes: usize,
    flushes: usize,
    shut: bool,
}

#[derive(Clone, Default)]
pub struct Probe {
    seen: Arc<Mutex<Seen>>,
}

impl Probe {
    pub fn new() -> Probe {
        Probe::default()
    }

    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.seen().data.clone()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.seen().data).into_owned()
    }

    pub fn lines(&self) -> usize {
        self.seen()
            .data
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    }

    pub fn writes(&self) -> usize {
        self.seen().writes
    }

    pub fn flushes(&self) -> usize {
        self.seen().flushes
    }

    pub fn is_shut(&self) -> bool {
        self.seen().shut
    }
}

impl AsyncWrite for Probe {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut seen = self.seen();
        if seen.shut {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        seen.data.extend_from_slice(buf);
        seen.writes += 1;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.seen().flushes += 1;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.seen().shut = true;
        Poll::Ready(Ok(()))
    }
}
