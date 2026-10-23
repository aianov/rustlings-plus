use std::fmt;
use std::io;

use crate::token::CancellationToken;
use crate::trail::Trail;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Terminate,
    Interrupt,
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Signal::Terminate => f.write_str("SIGTERM"),
            Signal::Interrupt => f.write_str("SIGINT"),
        }
    }
}

#[cfg(unix)]
pub struct Signals {
    term: tokio::signal::unix::Signal,
    int: tokio::signal::unix::Signal,
}

#[cfg(not(unix))]
pub struct Signals;

impl Signals {
    #[cfg(unix)]
    pub fn listen() -> io::Result<Signals> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Signals {
            term: signal(SignalKind::terminate())?,
            int: signal(SignalKind::interrupt())?,
        })
    }

    #[cfg(not(unix))]
    pub fn listen() -> io::Result<Signals> {
        Ok(Signals)
    }

    #[cfg(unix)]
    pub async fn next(&mut self) -> Signal {
        tokio::select! {
            _ = self.term.recv() => Signal::Terminate,
            _ = self.int.recv() => Signal::Interrupt,
        }
    }

    #[cfg(not(unix))]
    pub async fn next(&mut self) -> Signal {
        let _ = tokio::signal::ctrl_c().await;
        Signal::Interrupt
    }

    pub fn shutdown(mut self, trail: Trail) -> CancellationToken {
        let token = CancellationToken::new();
        let stop = token.clone();
        tokio::spawn(async move {
            let first = self.next().await;
            trail.note(&format!("получен {first}, останавливаюсь"));
            stop.cancel();
            loop {
                let again = self.next().await;
                trail.note(&format!(
                    "повторный {again}: остановка уже идёт и ограничена дедлайнами"
                ));
            }
        });
        token
    }
}
