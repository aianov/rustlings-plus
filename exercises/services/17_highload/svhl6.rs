// Инвалидация по событиям есть, но иногда после смены имени кэш на одном экземпляре всё равно показывает старое, и уже навсегда.
// Разбор: читатель сходил в базу за профилем, получил старую версию, и пока его ответ шёл по сети, пришло событие «профиль изменился»
// и вычистило кэш. Затем запоздавший читатель положил в кэш старое значение. Событие уже сработало, второго не будет.
// Цель: запись в кэш не должна перетирать более новое знание. События и записи в базе несут номер версии, и экземпляр кладёт значение
// в кэш только если его версия не старее последней известной о профиле.


use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

fn main() {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: u64,
    pub version: u64,
}

#[derive(Default)]
pub struct Db {
    rows: Mutex<HashMap<u64, (String, u64)>>,
    gate: Mutex<Option<oneshot::Receiver<()>>>,
}

impl Db {
    /// Меняет строку и возвращает событие, которое надо разослать.
    pub fn set(&self, id: u64, name: &str) -> Event {
        let mut rows = self.rows.lock().unwrap();
        let version = rows.get(&id).map_or(1, |(_, v)| v + 1);
        rows.insert(id, (name.to_string(), version));
        Event { id, version }
    }

    /// Следующее чтение возьмёт снимок и подождёт сигнала, прежде чем вернуть его (медленная сеть до базы).
    pub fn hold_next_read(&self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        *self.gate.lock().unwrap() = Some(rx);
        tx
    }

    async fn read(&self, id: u64) -> Option<(String, u64)> {
        let snapshot = self.rows.lock().unwrap().get(&id).cloned();
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.await;
        }
        snapshot
    }
}

pub struct Node {
    db: Arc<Db>,
    cache: Mutex<HashMap<u64, String>>,
    reads: AtomicU32,
}

impl Node {
    pub fn new(db: Arc<Db>) -> Arc<Node> {
        Arc::new(Node { db, cache: Mutex::new(HashMap::new()), reads: AtomicU32::new(0) })
    }

    pub fn db_reads(&self) -> u32 {
        self.reads.load(Ordering::SeqCst)
    }

    pub async fn get(&self, id: u64) -> Option<String> {
        if let Some(hit) = self.cache.lock().unwrap().get(&id) {
            return Some(hit.clone());
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        let (value, _version) = self.db.read(id).await?;
        self.cache.lock().unwrap().insert(id, value.clone());
        Some(value)
    }

    pub fn on_event(&self, ev: &Event) {
        self.cache.lock().unwrap().remove(&ev.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn settle() {
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn a_slow_reader_cannot_resurrect_old_data() {
        let db = Arc::new(Db::default());
        db.set(1, "Аня");
        let node = Node::new(db.clone());
        let release = db.hold_next_read();
        let slow = tokio::spawn({
            let node = node.clone();
            async move { node.get(1).await }
        });
        settle().await;
        let ev = db.set(1, "Анна");
        node.on_event(&ev);
        release.send(()).unwrap();
        slow.await.unwrap();
        settle().await;
        assert_eq!(node.get(1).await.as_deref(), Some("Анна"), "запоздавший читатель положил в кэш старое имя после инвалидации");
    }

    #[tokio::test]
    async fn the_event_may_arrive_before_the_reader_finishes_and_after() {
        let db = Arc::new(Db::default());
        db.set(1, "один");
        let node = Node::new(db.clone());
        let release = db.hold_next_read();
        let slow = tokio::spawn({
            let node = node.clone();
            async move { node.get(1).await }
        });
        settle().await;
        let ev2 = db.set(1, "два");
        let ev3 = db.set(1, "три");
        node.on_event(&ev3);
        node.on_event(&ev2);
        release.send(()).unwrap();
        slow.await.unwrap();
        assert_eq!(node.get(1).await.as_deref(), Some("три"), "события пришли не по порядку: известна версия 3, значит, версия 1 в кэш не годится");
    }

    #[tokio::test]
    async fn fresh_values_are_cached_as_before() {
        let db = Arc::new(Db::default());
        db.set(1, "Аня");
        let node = Node::new(db.clone());
        node.get(1).await;
        node.get(1).await;
        assert_eq!(node.db_reads(), 1, "обычное чтение кэшируется");
        let ev = db.set(1, "Анна");
        node.on_event(&ev);
        assert_eq!(node.get(1).await.as_deref(), Some("Анна"));
        node.get(1).await;
        assert_eq!(node.db_reads(), 2, "после инвалидации одно чтение из базы и снова кэш");
    }

    #[tokio::test]
    async fn an_old_event_does_not_evict_newer_data() {
        let db = Arc::new(Db::default());
        db.set(1, "один");
        let ev2 = db.set(1, "два");
        let node = Node::new(db.clone());
        node.get(1).await;
        node.on_event(&Event { id: 1, version: 1 });
        node.get(1).await;
        assert_eq!(node.db_reads(), 1, "событие о версии 1 опоздало: в кэше уже версия {}", ev2.version);
    }
}
