use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::time::Sleep;

fn count_from_thread(handle: &Handle, hits: Arc<AtomicUsize>) -> JoinHandle<()> {
    let handle = handle.clone();
    std::thread::Builder::new()
        .spawn(move || {
            handle.spawn(async move {
                hits.fetch_add(1, Ordering::SeqCst);
            });
        })
        .expect("не удалось запустить поток")
}

fn deadline(handle: &Handle, after: Duration) -> Sleep {
    let _guard = handle.enter();
    tokio::time::sleep(after)
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    count_from_thread(runtime.handle(), Arc::clone(&hits)).join().unwrap();
    runtime.block_on(deadline(runtime.handle(), Duration::from_millis(10)));
    println!("попаданий: {}", hits.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;
    use tokio::time::timeout;

    #[test]
    fn plain_thread_can_start_tasks() {
        let runtime = Runtime::new().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let thread = count_from_thread(runtime.handle(), Arc::clone(&hits));
        thread.join().expect("поток упал с паникой: в нём нет контекста рантайма, tokio::spawn оттуда вызывать нельзя");
        let seen = runtime.block_on(async {
            timeout(Duration::from_secs(2), async {
                while hits.load(Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .is_ok()
        });
        assert!(seen, "задача, запущенная из потока, так и не отработала");
    }

    #[test]
    fn sleep_can_be_created_in_sync_code() {
        let runtime = Runtime::new().unwrap();
        let sleep = deadline(runtime.handle(), Duration::from_millis(10));
        let finished = runtime.block_on(async { timeout(Duration::from_secs(2), sleep).await.is_ok() });
        assert!(finished, "Sleep должен дожить до ожидания в рантайме");
    }
}
