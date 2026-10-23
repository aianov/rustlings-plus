use std::io;
use tokio::signal::unix::{Signal as UnixSignal, SignalKind, signal};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Signal {
    Term,
    Int,
}

pub struct ShutdownListener {
    term: UnixSignal,
    int: UnixSignal,
}

impl ShutdownListener {
    pub fn new() -> io::Result<ShutdownListener> {
        Ok(ShutdownListener {
            term: signal(SignalKind::terminate())?,
            int: signal(SignalKind::interrupt())?,
        })
    }

    pub async fn wait(&mut self) -> Signal {
        tokio::select! {
            _ = self.term.recv() => Signal::Term,
            _ = self.int.recv() => Signal::Int,
        }
    }
}
