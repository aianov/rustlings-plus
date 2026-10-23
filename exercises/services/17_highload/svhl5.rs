// Сервис работает на трёх экземплярах, и у каждого свой кэш профилей в памяти. Пользователь сменил имя: экземпляр, который принял
// запрос, обновил базу и свой кэш, а два других ещё час (весь срок жизни записи) показывают старое имя. Срок жизни можно сократить,
// но проще сказать всем: «профиль 7 изменился». В системе есть шина событий, и каждый экземпляр на неё подписан.
// Цель: экземпляр убирает из кэша запись по событию и продолжает работать с остальными. Шина с ограниченной очередью может
// потерять события для отставшего подписчика, и тогда полагаться на кэш уже нельзя: он очищается целиком.


use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

fn main() {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: u64,
}

#[derive(Default)]
pub struct Db {
    rows: Mutex<HashMap<u64, String>>,
}

impl Db {
    pub fn set(&self, id: u64, name: &str) {
        self.rows.lock().unwrap().insert(id, name.to_string());
    }

    fn read(&self, id: u64) -> Option<String> {
        self.rows.lock().unwrap().get(&id).cloned()
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

    /// Сколько раз экземпляр ходил в базу.
    pub fn db_reads(&self) -> u32 {
        self.reads.load(Ordering::SeqCst)
    }

    pub async fn get(&self, id: u64) -> Option<String> {
        if let Some(hit) = self.cache.lock().unwrap().get(&id) {
            return Some(hit.clone());
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        let value = self.db.read(id)?;
        self.cache.lock().unwrap().insert(id, value.clone());
        Some(value)
    }

    /// Реакция на событие «профиль изменился».
    pub fn on_event(&self, ev: &Event) {
        let _ = ev;
        todo!()
    }

    /// Слушает шину, пока она не закроется.
    pub async fn listen(self: Arc<Node>, mut events: broadcast::Receiver<Event>) {
        let _ = &mut events;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn rename(db: &Db, bus: &broadcast::Sender<Event>, id: u64, name: &str) {
        db.set(id, name);
        bus.send(Event { id }).unwrap();
    }

    async fn settle() {
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn every_node_drops_the_changed_entry() {
        let db = Arc::new(Db::default());
        db.set(1, "Аня");
        db.set(2, "Боря");
        let (bus, _) = broadcast::channel(64);
        let nodes: Vec<_> = (0..3).map(|_| Node::new(db.clone())).collect();
        for n in &nodes {
            tokio::spawn(n.clone().listen(bus.subscribe()));
        }
        for n in &nodes {
            assert_eq!(n.get(1).await.as_deref(), Some("Аня"));
            assert_eq!(n.get(2).await.as_deref(), Some("Боря"));
        }
        rename(&db, &bus, 1, "Анна");
        settle().await;
        for n in &nodes {
            assert_eq!(n.get(1).await.as_deref(), Some("Анна"), "экземпляр показывает старое имя после события");
        }
    }

    #[tokio::test]
    async fn other_entries_stay_cached() {
        let db = Arc::new(Db::default());
        db.set(1, "Аня");
        db.set(2, "Боря");
        let (bus, _) = broadcast::channel(64);
        let node = Node::new(db.clone());
        tokio::spawn(node.clone().listen(bus.subscribe()));
        node.get(1).await;
        node.get(2).await;
        let before = node.db_reads();
        rename(&db, &bus, 1, "Анна");
        settle().await;
        node.get(2).await;
        assert_eq!(node.db_reads(), before, "событие про профиль 1 не должно выбрасывать профиль 2");
    }

    #[tokio::test]
    async fn a_lagging_listener_clears_everything() {
        let db = Arc::new(Db::default());
        for id in 1..=5 {
            db.set(id, "старое");
        }
        let (bus, _) = broadcast::channel(4);
        let node = Node::new(db.clone());
        for id in 1..=5 {
            node.get(id).await;
        }
        let rx = bus.subscribe();
        db.set(3, "новое");
        for _ in 0..20 {
            bus.send(Event { id: 3 }).unwrap();
        }
        for _ in 0..20 {
            bus.send(Event { id: 99 }).unwrap();
        }
        tokio::spawn(node.clone().listen(rx));
        settle().await;
        assert_eq!(node.get(3).await.as_deref(), Some("новое"), "подписчик отстал и потерял события: кэшу верить нельзя, его надо очистить целиком");
    }

    #[tokio::test]
    async fn the_listener_stops_when_the_bus_closes() {
        let db = Arc::new(Db::default());
        let (bus, rx) = broadcast::channel::<Event>(4);
        let task = tokio::spawn(Node::new(db).listen(rx));
        drop(bus);
        tokio::time::timeout(Duration::from_secs(2), task).await.expect("слушатель не завершился после закрытия шины").unwrap();
    }
}
