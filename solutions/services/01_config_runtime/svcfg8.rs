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

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut listener = ShutdownListener::new()?;
    println!("жду сигнал, pid {}", std::process::id());
    println!("получен {:?}", listener.wait().await);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn send(signal: &str) {
        let status = std::process::Command::new("kill")
            .args([signal, &std::process::id().to_string()])
            .status()
            .expect("не удалось запустить kill");
        assert!(status.success(), "kill {signal} завершился с ошибкой");
    }

    #[tokio::test]
    async fn both_signals_are_recognized() {
        let mut listener = ShutdownListener::new().expect("подписка на сигналы");

        send("-TERM");
        let got = tokio::time::timeout(Duration::from_secs(5), listener.wait()).await;
        assert_eq!(got, Ok(Signal::Term), "SIGTERM пришёл до первого wait: подписка должна жить с момента new(), иначе сигнал убьёт процесс или потеряется");

        let quiet = tokio::time::timeout(Duration::from_millis(100), listener.wait()).await;
        assert!(quiet.is_err(), "без нового сигнала wait не должен возвращаться");

        send("-INT");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let got = tokio::time::timeout(Duration::from_secs(5), listener.wait()).await;
        assert_eq!(got, Ok(Signal::Int), "SIGINT (Ctrl+C) должен давать Signal::Int");
    }
}
