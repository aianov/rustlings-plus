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
pub struct PostRow {
    pub id: String,
    pub author_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRow {
    pub seq: u64,
    pub event: Event,
    pub sent: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub posts: Vec<PostRow>,
    pub outbox: Vec<OutboxRow>,
}

#[derive(Debug, PartialEq)]
pub enum DbError {
    CommitFailed,
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    Invalid(&'static str),
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

pub struct PostService {
    db: Arc<Db>,
    ids: Arc<dyn Ids + Send + Sync>,
}

impl PostService {
    pub fn new(db: Arc<Db>, ids: Arc<dyn Ids + Send + Sync>) -> PostService {
        PostService { db, ids }
    }

    pub fn create_post(&self, now: u64, author_id: &str, text: &str) -> Result<String, PostError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(PostError::Invalid("text"));
        }
        if text.chars().count() > 280 {
            return Err(PostError::Invalid("text"));
        }
        let post_id = self.ids.next_id();
        let event = Event {
            event_id: self.ids.next_id(),
            kind: "post.created".to_string(),
            schema: 1,
            version: 1,
            occurred_at: now,
            aggregate_id: post_id.clone(),
            payload: json!({"post_id": post_id, "author_id": author_id, "text": text}),
        };
        let row = PostRow { id: post_id.clone(), author_id: author_id.to_string(), text: text.to_string() };
        self.db
            .transaction(|tables| {
                tables.posts.push(row);
                let seq = tables.outbox.len() as u64 + 1;
                tables.outbox.push(OutboxRow { seq, event, sent: false });
                Ok(())
            })
            .map_err(PostError::Db)?;
        Ok(post_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (Arc<Db>, PostService) {
        let db = Arc::new(Db::new());
        (db.clone(), PostService::new(db, Arc::new(Seq::new())))
    }

    #[test]
    fn a_post_and_its_event_are_saved_together() {
        let (db, service) = service();
        let id = service.create_post(1000, "alice", "  привет  ").expect("пост создан");
        let tables = db.snapshot();
        assert_eq!(tables.posts, vec![PostRow { id: id.clone(), author_id: "alice".into(), text: "привет".into() }], "пост сохранён без пробелов по краям");
        assert_eq!(tables.outbox.len(), 1, "и ровно одна строка outbox");
        let row = &tables.outbox[0];
        assert_eq!((row.seq, row.sent), (1, false), "seq с единицы, ещё не отправлено");
        assert_eq!((row.event.kind.as_str(), row.event.version, row.event.schema), ("post.created", 1, 1), "тип, версия агрегата и схема");
        assert_eq!((row.event.aggregate_id.as_str(), row.event.occurred_at), (id.as_str(), 1000), "агрегат это пост, время то, что передали");
        assert_eq!(row.event.payload["text"], "привет", "текст в событии тот же, что в таблице");
        assert_ne!(row.event.event_id, id, "идентификатор события не равен идентификатору поста");
    }

    #[test]
    fn seq_grows_with_every_post() {
        let (db, service) = service();
        service.create_post(1, "alice", "один").unwrap();
        service.create_post(2, "bob", "два").unwrap();
        let seqs: Vec<u64> = db.snapshot().outbox.iter().map(|row| row.seq).collect();
        assert_eq!(seqs, vec![1, 2], "порядок публикации задаёт seq");
    }

    #[test]
    fn a_failed_commit_leaves_neither_the_post_nor_the_event() {
        let (db, service) = service();
        db.fail_commit_number(1);
        let result = service.create_post(1, "alice", "текст");
        assert_eq!(result, Err(PostError::Db(DbError::CommitFailed)), "ошибка базы возвращается вызывающему");
        let tables = db.snapshot();
        assert!(tables.posts.is_empty() && tables.outbox.is_empty(), "после неудачной записи нет ни поста, ни события: {tables:?}");
    }

    #[test]
    fn one_post_is_one_transaction() {
        let (db, service) = service();
        db.fail_commit_number(2);
        let result = service.create_post(1, "alice", "текст");
        assert!(result.is_ok(), "транзакция должна быть одна: второй коммит, который сломали, не должен случиться, а там их было два: {result:?}");
        let tables = db.snapshot();
        assert_eq!((tables.posts.len(), tables.outbox.len()), (1, 1), "пост и событие всегда идут парой");
    }

    #[test]
    fn invalid_text_writes_nothing_at_all() {
        let (db, service) = service();
        let too_long = "я".repeat(281);
        for text in ["", "   ", too_long.as_str()] {
            assert!(matches!(service.create_post(1, "alice", text), Err(PostError::Invalid("text"))), "текст {:?} не годится", text.chars().take(5).collect::<String>());
        }
        let tables = db.snapshot();
        assert!(tables.posts.is_empty() && tables.outbox.is_empty(), "ничего не записано");
        assert!(service.create_post(1, "alice", &"я".repeat(280)).is_ok(), "280 символов это 560 байтов, считаются символы");
    }
}
