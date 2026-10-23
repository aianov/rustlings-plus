use crate::{Db, DbError, OutboxRow, Tables, UserRow};
use events::Event;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait Ids: Send + Sync {
    fn next_id(&self) -> String;
}

#[derive(Default)]
pub struct SeqIds(AtomicU64);

impl SeqIds {
    pub fn new() -> SeqIds {
        SeqIds::default()
    }
}

impl Ids for SeqIds {
    fn next_id(&self) -> String {
        format!("evt-{}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

#[derive(Debug, PartialEq)]
pub enum UserError {
    Invalid(&'static str),
    AlreadyExists,
    NotFound,
    SelfFollow,
    Db(DbError),
}

pub struct UserService {
    db: Arc<Db>,
    ids: Arc<dyn Ids>,
}

fn push_event(tables: &mut Tables, event_id: String, kind: &str, schema: u32, now: u64, aggregate: &str, payload: Value) {
    let version = tables.outbox.iter().filter(|row| row.event.aggregate_id == aggregate).count() as u64 + 1;
    let seq = tables.outbox.len() as u64 + 1;
    tables.outbox.push(OutboxRow { seq, event: Event::new(event_id, kind, schema, version, now, aggregate, payload), sent: false });
}

impl UserService {
    pub fn new(db: Arc<Db>, ids: Arc<dyn Ids>) -> UserService {
        UserService { db, ids }
    }

    pub fn register(&self, now: u64, user_id: &str, name: &str) -> Result<(), UserError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 50 {
            return Err(UserError::Invalid("name"));
        }
        let event_id = self.ids.next_id();
        self.db
            .transaction(|tables| {
                if tables.users.iter().any(|user| user.id == user_id) {
                    return Err(UserError::AlreadyExists);
                }
                tables.users.push(UserRow { id: user_id.to_string(), name: name.to_string() });
                push_event(tables, event_id, "user.registered", 1, now, user_id, json!({"user_id": user_id, "name": name}));
                Ok(())
            })
    }

    pub fn rename(&self, now: u64, user_id: &str, first_name: &str, last_name: &str) -> Result<(), UserError> {
        let (first, last) = (first_name.trim(), last_name.trim());
        if first.is_empty() || first.chars().count() > 50 || last.chars().count() > 50 {
            return Err(UserError::Invalid("name"));
        }
        let event_id = self.ids.next_id();
        self.db
            .transaction(|tables| {
                let Some(user) = tables.users.iter_mut().find(|user| user.id == user_id) else {
                    return Err(UserError::NotFound);
                };
                user.name = format!("{first} {last}").trim().to_string();
                push_event(tables, event_id, "user.profile_changed", 2, now, user_id, json!({"first_name": first, "last_name": last}));
                Ok(())
            })
    }

    pub fn follow(&self, now: u64, follower: &str, followee: &str) -> Result<bool, UserError> {
        if follower == followee {
            return Err(UserError::SelfFollow);
        }
        let event_id = self.ids.next_id();
        self.db
            .transaction(|tables| {
                let known = |id: &str| tables.users.iter().any(|user| user.id == id);
                if !known(follower) || !known(followee) {
                    return Err(UserError::NotFound);
                }
                let pair = (follower.to_string(), followee.to_string());
                if tables.follows.contains(&pair) {
                    return Ok(false);
                }
                tables.follows.push(pair);
                push_event(tables, event_id, "user.followed", 1, now, follower, json!({"follower_id": follower, "followee_id": followee}));
                Ok(true)
            })
    }
}

impl From<DbError> for UserError {
    fn from(error: DbError) -> UserError {
        UserError::Db(error)
    }
}
