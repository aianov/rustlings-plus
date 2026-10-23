// Если выбросить JoinHandle, задача не отменяется: она отсоединяется и живёт дальше сама.
// Иногда это нужно, но чаще хочется обратного: владелец задачи ушёл, и задача должна уйти
// вместе с ним. В Rust за такое отвечает Drop, а готовой обёртки в tokio нет (без tokio-util).
// TODO: Допиши Drop для AbortOnDrop: при уничтожении обёртки задача отменяется.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;

struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> AbortOnDrop<T> {
    fn new(handle: JoinHandle<T>) -> Self {
        AbortOnDrop(handle)
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

fn spawn_ticker(counter: Arc<AtomicUsize>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {}
}

#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let guard = AbortOnDrop::new(spawn_ticker(Arc::clone(&counter)));
        tokio::time::sleep(Duration::from_millis(35)).await;
        println!("закончена: {}", guard.is_finished());
    }
    println!("тиков: {}", counter.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn plain_handle_drop_detaches() {
        let counter = Arc::new(AtomicUsize::new(0));
        drop(spawn_ticker(Arc::clone(&counter)));
        tokio::time::sleep(Duration::from_millis(35)).await;
        let before = counter.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            counter.load(Ordering::SeqCst) > before,
            "обычный drop JoinHandle не отменяет задачу, тикер должен продолжать"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn guard_drop_cancels() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guard = AbortOnDrop::new(spawn_ticker(Arc::clone(&counter)));
        tokio::time::sleep(Duration::from_millis(35)).await;
        assert!(!guard.is_finished(), "пока обёртка жива, задача работает");
        drop(guard);
        tokio::task::yield_now().await;
        let frozen = counter.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            counter.load(Ordering::SeqCst),
            frozen,
            "обёртка уничтожена, а тикер всё ещё тикает: Drop должен вызывать abort"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn leaving_a_scope_cancels() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let _guard = AbortOnDrop::new(spawn_ticker(Arc::clone(&counter)));
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        tokio::task::yield_now().await;
        let frozen = counter.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), frozen, "выход из области видимости должен останавливать задачу");
    }
}
