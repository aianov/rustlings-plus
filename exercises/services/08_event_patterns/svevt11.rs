// user-service при подписке сначала публикует `user.followed` в шину, а потом пишет подписку в базу. Если база
// отказала или такая подписка уже была, событие уже улетело, и лента показывает подписку, которой нет. Цель: событие
// существует тогда и только тогда, когда подписка действительно записана. Подписать на себя нельзя (SelfFollow),
// повторная подписка ничего не пишет и не создаёт события (Ok(false)). Версия события это номер события в агрегате:
// агрегат здесь подписчик, и у его первого события версия 1, у второго 2. Шина сервису больше не нужна, её можно убрать.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn main() {}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "one")]
    pub schema: u32,
    pub version: u64,
    pub occurred_at: u64,
    pub aggregate_id: String,
    pub payload: Value,
}

pub trait Ids {
    fn next_id(&self) -> String;
}

pub struct Seq(std::sync::atomic::AtomicU64);

impl Seq {
    pub fn new() -> Seq {
        Seq(std::sync::atomic::AtomicU64::new(0))
    }
}

impl Default for Seq {
    fn default() -> Seq {
        Seq::new()
    }
}

impl Ids for Seq {
    fn next_id(&self) -> String {
        format!("id-{}", self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRow {
    pub seq: u64,
    pub event: Event,
    pub sent: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub follows: Vec<(String, String)>,
    pub outbox: Vec<OutboxRow>,
}

#[derive(Debug, PartialEq)]
pub enum DbError {
    CommitFailed,
}

#[derive(Debug, PartialEq)]
pub enum FollowError {
    SelfFollow,
    Db(DbError),
}

#[derive(Default)]
pub struct Db {
    tables: Mutex<Tables>,
    commits: AtomicUsize,
    fail_on: Mutex<Option<usize>>,
}

impl Db {
    pub fn new() -> Db {
        Db::default()
    }

    pub fn fail_commit_number(&self, number: usize) {
        *self.fail_on.lock().unwrap() = Some(number);
    }

    pub fn snapshot(&self) -> Tables {
        self.tables.lock().unwrap().clone()
    }

    pub fn transaction<R>(&self, work: impl FnOnce(&mut Tables) -> Result<R, DbError>) -> Result<R, DbError> {
        let mut tables = self.tables.lock().unwrap();
        let mut staged = tables.clone();
        let result = work(&mut staged)?;
        let number = self.commits.fetch_add(1, Ordering::SeqCst) + 1;
        if *self.fail_on.lock().unwrap() == Some(number) {
            return Err(DbError::CommitFailed);
        }
        *tables = staged;
        Ok(result)
    }
}

#[derive(Default)]
pub struct Bus {
    published: Mutex<Vec<Event>>,
}

impl Bus {
    pub fn publish(&self, event: Event) {
        self.published.lock().unwrap().push(event);
    }
}

pub struct FollowService {
    db: Arc<Db>,
    bus: Bus,
    ids: Arc<dyn Ids + Send + Sync>,
}

impl FollowService {
    pub fn new(db: Arc<Db>, ids: Arc<dyn Ids + Send + Sync>) -> FollowService {
        FollowService { db, bus: Bus::default(), ids }
    }

    pub fn follow(&self, now: u64, follower: &str, followee: &str) -> Result<bool, FollowError> {
        let event = Event {
            event_id: self.ids.next_id(),
            kind: "user.followed".to_string(),
            schema: 1,
            version: 1,
            occurred_at: now,
            aggregate_id: follower.to_string(),
            payload: json!({"follower_id": follower, "followee_id": followee}),
        };
        self.bus.publish(event);
        self.db
            .transaction(|tables| {
                let pair = (follower.to_string(), followee.to_string());
                if tables.follows.contains(&pair) {
                    return Ok(false);
                }
                tables.follows.push(pair);
                Ok(true)
            })
            .map_err(FollowError::Db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (Arc<Db>, FollowService) {
        let db = Arc::new(Db::new());
        (db.clone(), FollowService::new(db, Arc::new(Seq::new())))
    }

    #[test]
    fn a_subscription_and_its_event_appear_together() {
        let (db, service) = service();
        assert_eq!(service.follow(77, "bob", "alice"), Ok(true), "подписка записана");
        let tables = db.snapshot();
        assert_eq!(tables.follows, vec![("bob".to_string(), "alice".to_string())], "пара в таблице");
        assert_eq!(tables.outbox.len(), 1, "и событие в outbox");
        let event = &tables.outbox[0].event;
        assert_eq!((event.kind.as_str(), event.aggregate_id.as_str(), event.version, event.occurred_at), ("user.followed", "bob", 1, 77), "конверт");
        assert_eq!(event.payload, json!({"follower_id": "bob", "followee_id": "alice"}), "payload");
    }

    #[test]
    fn a_repeated_subscription_makes_no_second_event() {
        let (db, service) = service();
        service.follow(1, "bob", "alice").unwrap();
        assert_eq!(service.follow(2, "bob", "alice"), Ok(false), "повтор это не ошибка, но и не новость");
        let tables = db.snapshot();
        assert_eq!((tables.follows.len(), tables.outbox.len()), (1, 1), "ни второй подписки, ни второго события");
    }

    #[test]
    fn a_failed_write_produces_no_event() {
        let (db, service) = service();
        db.fail_commit_number(1);
        assert_eq!(service.follow(1, "bob", "alice"), Err(FollowError::Db(DbError::CommitFailed)), "ошибка базы");
        let tables = db.snapshot();
        assert!(tables.follows.is_empty() && tables.outbox.is_empty(), "нет подписки, значит нет и события: {tables:?}");
    }

    #[test]
    fn following_yourself_is_rejected_before_anything_is_written() {
        let (db, service) = service();
        assert_eq!(service.follow(1, "bob", "bob"), Err(FollowError::SelfFollow), "на себя нельзя");
        let tables = db.snapshot();
        assert!(tables.follows.is_empty() && tables.outbox.is_empty(), "и следа нет");
    }

    #[test]
    fn versions_count_the_events_of_each_subscriber_separately() {
        let (db, service) = service();
        service.follow(1, "bob", "alice").unwrap();
        service.follow(2, "carol", "alice").unwrap();
        service.follow(3, "bob", "dave").unwrap();
        service.follow(4, "bob", "alice").unwrap();
        let versions: Vec<(String, u64)> = db.snapshot().outbox.iter().map(|row| (row.event.aggregate_id.clone(), row.event.version)).collect();
        assert_eq!(
            versions,
            vec![("bob".to_string(), 1), ("carol".to_string(), 1), ("bob".to_string(), 2)],
            "у bob версии 1 и 2, у carol своя первая, а повтор версию не расходует"
        );
    }
}
