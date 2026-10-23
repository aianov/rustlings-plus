use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Cache {
    map: Mutex<HashMap<u32, String>>,
    loads: AtomicUsize,
}

async fn load(id: u32, loads: &AtomicUsize) -> String {
    loads.fetch_add(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(10)).await;
    format!("значение {id}")
}

async fn get_or_load(cache: Arc<Cache>, id: u32) -> String {
    let cached = cache.map.lock().unwrap().get(&id).cloned();
    match cached {
        Some(value) => value,
        None => {
            let value = load(id, &cache.loads).await;
            cache.map.lock().unwrap().insert(id, value.clone());
            value
        }
    }
}

#[tokio::main]
async fn main() {
    let cache = Arc::new(Cache::default());
    println!("{}", get_or_load(Arc::clone(&cache), 1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn loads_then_caches() {
        let cache = Arc::new(Cache::default());
        let first = timeout(Duration::from_secs(1), tokio::spawn(get_or_load(Arc::clone(&cache), 7)))
            .await
            .expect("get_or_load завис: замок взят дважды в одной задаче")
            .unwrap();
        assert_eq!(first, "значение 7", "первая загрузка");
        let second = tokio::spawn(get_or_load(Arc::clone(&cache), 7)).await.unwrap();
        assert_eq!(second, "значение 7", "второй раз то же значение");
        assert_eq!(cache.loads.load(Ordering::SeqCst), 1, "второй запрос должен прийти из кэша");
    }

    #[tokio::test(start_paused = true)]
    async fn different_ids() {
        let cache = Arc::new(Cache::default());
        let a = tokio::spawn(get_or_load(Arc::clone(&cache), 1));
        let b = tokio::spawn(get_or_load(Arc::clone(&cache), 2));
        assert_eq!(a.await.unwrap(), "значение 1");
        assert_eq!(b.await.unwrap(), "значение 2");
        assert_eq!(cache.loads.load(Ordering::SeqCst), 2, "два разных ключа это две загрузки");
    }
}
