// abort не убивает поток и не прерывает код посередине. Он помечает задачу, и она
// останавливается при следующем возвращении в рантайм, то есть в точке .await. Цикл без
// единого .await не отменить: пока он крутится, abort ждёт. Здесь тяжёлая задача считает
// 600 мс подряд, и тест ждёт, что после abort она остановится быстро.
// TODO: Сделай задачу отменяемой, не меняя смысла цикла.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

async fn crunch(counter: Arc<AtomicU64>) {
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicU64::new(0));
    let handle = tokio::spawn(crunch(Arc::clone(&counter)));
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();
    println!("{:?}", handle.await.is_err());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abort_stops_the_loop_quickly() {
        let counter = Arc::new(AtomicU64::new(0));
        let handle = tokio::spawn(crunch(Arc::clone(&counter)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.abort();
        let joined = timeout(Duration::from_millis(200), handle).await.expect(
            "задача не остановилась за 200 мс после abort: в цикле нет ни одного .await, и отмене негде сработать",
        );
        assert!(joined.unwrap_err().is_cancelled(), "задачу должны были отменить, а не дождаться конца");
        let frozen = counter.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::Relaxed), frozen, "после отмены счётчик не должен расти");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn without_abort_it_still_finishes() {
        let counter = Arc::new(AtomicU64::new(0));
        tokio::spawn(crunch(Arc::clone(&counter))).await.unwrap();
        assert!(counter.load(Ordering::Relaxed) > 0, "без отмены задача работает до дедлайна");
    }
}
