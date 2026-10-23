use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

/// Одновременные вызовы с одним ключом выполняют загрузчик один раз и делят результат.
pub struct SingleFlight<V: Clone> {
    inflight: Mutex<HashMap<String, Arc<OnceCell<V>>>>,
}

impl<V: Clone> Default for SingleFlight<V> {
    fn default() -> SingleFlight<V> {
        SingleFlight { inflight: Mutex::new(HashMap::new()) }
    }
}

impl<V: Clone> SingleFlight<V> {
    pub async fn run<Fut: Future<Output = V>>(&self, key: &str, load: impl FnOnce() -> Fut) -> V {
        let cell = self.inflight.lock().unwrap().entry(key.to_string()).or_default().clone();
        let value = cell.get_or_init(load).await.clone();
        let mut map = self.inflight.lock().unwrap();
        if map.get(key).is_some_and(|current| Arc::ptr_eq(current, &cell)) {
            map.remove(key);
        }
        value
    }
}
