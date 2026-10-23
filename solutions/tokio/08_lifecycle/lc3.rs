use futures::future::select_all;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

#[allow(dead_code)]
impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
    }
}

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

async fn heartbeat(token: Token, every: Duration, beats: Arc<AtomicU32>) {
    let mut tick = tokio::time::interval(every);
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            _ = tick.tick() => {
                beats.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let token = Token::new();
    let beats = Arc::new(AtomicU32::new(0));
    let task = tokio::spawn(heartbeat(token.clone(), Duration::from_millis(50), beats.clone()));
    tokio::time::sleep(Duration::from_millis(120)).await;
    token.cancel();
    task.await.unwrap();
    println!("{}", beats.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn beats_every_interval_and_stops_at_once() {
        let token = Token::new();
        let beats = Arc::new(AtomicU32::new(0));
        let task = tokio::spawn(heartbeat(token.clone(), Duration::from_millis(100), beats.clone()));
        sleep(Duration::from_millis(350)).await;
        assert_eq!(beats.load(Ordering::SeqCst), 4, "удары в 0, 100, 200 и 300 мс: к 350 мс их четыре");
        let cancelled_at = Instant::now();
        token.cancel();
        timeout(Duration::from_secs(1), task).await.expect("после отмены задача так и не завершилась").unwrap();
        assert_eq!(cancelled_at.elapsed(), Duration::ZERO, "остановка должна быть мгновенной, а не по следующему тику");
        sleep(Duration::from_millis(500)).await;
        assert_eq!(beats.load(Ordering::SeqCst), 4, "после остановки ударов быть не должно");
    }

    #[tokio::test(start_paused = true)]
    async fn an_already_cancelled_token_means_no_beats() {
        let token = Token::new();
        token.cancel();
        let beats = Arc::new(AtomicU32::new(0));
        let task = tokio::spawn(heartbeat(token, Duration::from_millis(100), beats.clone()));
        timeout(Duration::from_secs(1), task).await.expect("задача с отменённым токеном должна сразу выйти").unwrap();
        assert_eq!(beats.load(Ordering::SeqCst), 0, "токен отменён до старта: ни одного удара");
    }

    #[tokio::test(start_paused = true)]
    async fn a_child_token_stops_it_too() {
        let root = Token::new();
        let beats = Arc::new(AtomicU32::new(0));
        let task = tokio::spawn(heartbeat(root.child(), Duration::from_millis(100), beats.clone()));
        sleep(Duration::from_millis(250)).await;
        root.cancel();
        timeout(Duration::from_secs(1), task).await.expect("отмена родителя должна остановить задачу ребёнка").unwrap();
        assert_eq!(beats.load(Ordering::SeqCst), 3, "удары в 0, 100 и 200 мс");
    }
}
