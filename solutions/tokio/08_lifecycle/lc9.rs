use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

struct Sink {
    lines: Mutex<Vec<String>>,
    writes: AtomicUsize,
}

impl Sink {
    fn new() -> Arc<Sink> {
        Arc::new(Sink { lines: Mutex::new(Vec::new()), writes: AtomicUsize::new(0) })
    }

    async fn write_all(&self, lines: Vec<String>) {
        sleep(Duration::from_millis(10)).await;
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.lines.lock().unwrap().extend(lines);
    }
}

async fn drain_fallback(mut rx: mpsc::UnboundedReceiver<Vec<String>>, sink: Arc<Sink>) {
    while let Some(lines) = rx.recv().await {
        sink.write_all(lines).await;
    }
}

struct Session {
    buffer: Vec<String>,
    sink: Arc<Sink>,
    fallback: mpsc::UnboundedSender<Vec<String>>,
}

impl Session {
    fn new(sink: Arc<Sink>, fallback: mpsc::UnboundedSender<Vec<String>>) -> Session {
        Session { buffer: Vec::new(), sink, fallback }
    }

    fn log(&mut self, line: &str) {
        self.buffer.push(line.to_string());
    }

    async fn close(mut self) {
        let lines = std::mem::take(&mut self.buffer);
        if !lines.is_empty() {
            self.sink.write_all(lines).await;
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let lines = std::mem::take(&mut self.buffer);
        if !lines.is_empty() {
            let _ = self.fallback.send(lines);
        }
    }
}

#[tokio::main]
async fn main() {
    let sink = Sink::new();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(drain_fallback(rx, sink.clone()));
    let mut session = Session::new(sink.clone(), tx);
    session.log("привет");
    session.close().await;
    println!("{:?}", sink.lines.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Sink>, mpsc::UnboundedSender<Vec<String>>) {
        let sink = Sink::new();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(drain_fallback(rx, sink.clone()));
        (sink, tx)
    }

    #[tokio::test(start_paused = true)]
    async fn explicit_close_writes_once_and_drop_adds_nothing() {
        let (sink, tx) = setup();
        let mut session = Session::new(sink.clone(), tx);
        session.log("a");
        session.log("b");
        session.close().await;
        sleep(Duration::from_millis(100)).await;
        assert_eq!(*sink.lines.lock().unwrap(), vec!["a", "b"], "строки должны попасть в приёмник по одному разу");
        assert_eq!(sink.writes.load(Ordering::SeqCst), 1, "после close запись одна: Drop не должен писать то же второй раз");
    }

    #[tokio::test(start_paused = true)]
    async fn a_forgotten_close_still_delivers_the_lines() {
        let (sink, tx) = setup();
        let mut session = Session::new(sink.clone(), tx);
        session.log("x");
        session.log("y");
        drop(session);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(*sink.lines.lock().unwrap(), vec!["x", "y"], "сессию выбросили без close: строки должны дойти запасным путём");
        assert_eq!(sink.writes.load(Ordering::SeqCst), 1, "запасной путь пишет ровно один раз");
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_session_writes_nothing() {
        let (sink, tx) = setup();
        let session = Session::new(sink.clone(), tx);
        drop(session);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(sink.writes.load(Ordering::SeqCst), 0, "пустая сессия: незачем писать пустую пачку");
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_after_the_background_task_is_gone_does_not_panic() {
        let sink = Sink::new();
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);
        let mut session = Session::new(sink.clone(), tx);
        session.log("поздно");
        drop(session);
        assert!(sink.lines.lock().unwrap().is_empty(), "запасной путь закрыт: строки потеряны, но паники быть не должно");
    }
}
