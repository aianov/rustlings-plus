use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

fn slow_hash(password: &str) -> String {
    std::thread::sleep(Duration::from_millis(100));
    format!("hash:{}", password.chars().rev().collect::<String>())
}

async fn heartbeat(beats: Arc<AtomicUsize>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        beats.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn hash_with_heartbeat(password: &str) -> (String, usize) {
    let beats = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let pulse = tokio::spawn(heartbeat(Arc::clone(&beats), Arc::clone(&stop)));
    let hash = hash_password(password.to_string()).await;
    stop.store(true, Ordering::SeqCst);
    pulse.await.unwrap();
    (hash, beats.load(Ordering::SeqCst))
}

async fn hash_password(password: String) -> String {
    tokio::task::spawn_blocking(move || slow_hash(&password)).await.unwrap()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (hash, beats) = hash_with_heartbeat("секрет").await;
    println!("{hash}, тиков: {beats}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_is_correct() {
        assert_eq!(hash_password("abc".to_string()).await, "hash:cba", "результат slow_hash должен дойти без изменений");
    }

    #[tokio::test]
    async fn heartbeat_survives_hashing() {
        let (_, beats) = hash_with_heartbeat("abc").await;
        assert!(
            beats >= 10,
            "за время хеширования сердце тикнуло {beats} раз(а), а ждали около 20: slow_hash держит поток рантайма. Унеси её в spawn_blocking"
        );
    }

    #[tokio::test]
    async fn several_hashes_in_parallel() {
        let started = std::time::Instant::now();
        let handles: Vec<_> = (0..4).map(|i| tokio::spawn(hash_password(format!("p{i}")))).collect();
        for handle in handles {
            handle.await.unwrap();
        }
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "четыре хеша по 100 мс заняли {:?}: блокирующий пул должен считать их параллельно",
            started.elapsed()
        );
    }
}
