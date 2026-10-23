use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

fn spawn_pipeline(counter: Arc<AtomicU32>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u32>();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if tx.send(1).is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(n) = rx.recv().await {
            counter.fetch_add(n, Ordering::SeqCst);
        }
    });
}

async fn tick_forward(d: Duration) {
    tokio::time::advance(d).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let counter = Arc::new(AtomicU32::new(0));
    spawn_pipeline(Arc::clone(&counter));
    tick_forward(Duration::ZERO).await;
    tick_forward(Duration::from_millis(100)).await;
    println!("{}", counter.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn counter_follows_every_step() {
        let counter = Arc::new(AtomicU32::new(0));
        spawn_pipeline(Arc::clone(&counter));
        tick_forward(Duration::ZERO).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "до первого тика счётчик пуст");
        for expected in 1..=5 {
            tick_forward(Duration::from_millis(100)).await;
            assert_eq!(
                counter.load(Ordering::SeqCst),
                expected,
                "после шага {expected} задачи не успели отработать: advance сдвигает часы, но не ждёт задач"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn time_does_not_move_by_itself() {
        let counter = Arc::new(AtomicU32::new(0));
        spawn_pipeline(Arc::clone(&counter));
        tick_forward(Duration::ZERO).await;
        tick_forward(Duration::from_millis(99)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "за 99 мс тика ещё не было");
        tick_forward(Duration::from_millis(1)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1, "на 100-й миллисекунде тик пришёл");
    }
}
