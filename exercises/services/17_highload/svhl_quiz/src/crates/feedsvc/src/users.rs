use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Подмена user-service: вызов пачкой стоит `latency`, за раз не больше 100 идентификаторов.
pub struct Users {
    names: Mutex<HashMap<u64, String>>,
    latency: Duration,
    batch_calls: AtomicU32,
    ids_asked: AtomicU32,
}

impl Users {
    pub fn new(names: HashMap<u64, String>, latency: Duration) -> Users {
        Users { names: Mutex::new(names), latency, batch_calls: AtomicU32::new(0), ids_asked: AtomicU32::new(0) }
    }

    pub fn rename(&self, id: u64, name: &str) {
        self.names.lock().unwrap().insert(id, name.to_string());
    }

    pub fn batch_calls(&self) -> u32 {
        self.batch_calls.load(Ordering::SeqCst)
    }

    pub fn ids_asked(&self) -> u32 {
        self.ids_asked.load(Ordering::SeqCst)
    }

    pub async fn get_many(&self, ids: &[u64]) -> HashMap<u64, String> {
        assert!(ids.len() <= 100, "за один вызов нельзя спрашивать больше 100 идентификаторов");
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        self.ids_asked.fetch_add(ids.len() as u32, Ordering::SeqCst);
        tokio::time::sleep(self.latency).await;
        let names = self.names.lock().unwrap();
        ids.iter().filter_map(|id| names.get(id).map(|n| (*id, n.clone()))).collect()
    }
}
