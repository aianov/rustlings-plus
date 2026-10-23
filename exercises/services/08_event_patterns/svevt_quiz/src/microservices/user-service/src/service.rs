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

impl UserService {
    pub fn new(db: Arc<Db>, ids: Arc<dyn Ids>) -> UserService {
        UserService { db, ids }
    }

    pub fn register(&self, now: u64, user_id: &str, name: &str) -> Result<(), UserError> {
        todo!()
    }

    pub fn rename(&self, now: u64, user_id: &str, first_name: &str, last_name: &str) -> Result<(), UserError> {
        todo!()
    }

    pub fn follow(&self, now: u64, follower: &str, followee: &str) -> Result<bool, UserError> {
        todo!()
    }
}

impl From<DbError> for UserError {
    fn from(error: DbError) -> UserError {
        UserError::Db(error)
    }
}
