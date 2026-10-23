use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::{AbortHandle, JoinHandle};

fn spawn_ticker(counter: Arc<AtomicUsize>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

async fn stop(handle: JoinHandle<()>) -> bool {
    handle.abort();
    handle.await.is_err_and(|error| error.is_cancelled())
}

fn killer(handle: &JoinHandle<()>) -> AbortHandle {
    handle.abort_handle()
}

#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicUsize::new(0));
    let handle = spawn_ticker(Arc::clone(&counter));
    let remote = killer(&handle);
    tokio::time::sleep(Duration::from_millis(35)).await;
    remote.abort();
    println!("отменена: {}", stop(handle).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn stop_cancels_the_task() {
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = spawn_ticker(Arc::clone(&counter));
        tokio::time::sleep(Duration::from_millis(35)).await;
        assert!(stop(handle).await, "после abort ожидание хэндла должно дать JoinError с is_cancelled");
        let frozen = counter.load(Ordering::SeqCst);
        assert!(frozen >= 3, "до остановки тикер успел тикнуть несколько раз, а вышло {frozen}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), frozen, "остановленная задача больше не тикает");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_reports_false_for_a_finished_task() {
        let handle = tokio::spawn(async {});
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(!stop(handle).await, "завершённую задачу отменить нельзя, её никто не отменял");
    }

    #[tokio::test(start_paused = true)]
    async fn abort_handle_works_from_elsewhere() {
        let counter = Arc::new(AtomicUsize::new(0));
        let handle = spawn_ticker(Arc::clone(&counter));
        let remote = killer(&handle);
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!remote.is_finished(), "пока задача работает, is_finished ложно");
        remote.abort();
        let error = handle.await.unwrap_err();
        assert!(error.is_cancelled(), "отмена через AbortHandle видна в хэндле как cancelled");
        assert!(remote.is_finished(), "после отмены пульт сообщает, что задача закончилась");
    }
}
