use events::Event;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub struct UserRow {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRow {
    pub seq: u64,
    pub event: Event,
    pub sent: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub users: Vec<UserRow>,
    pub follows: Vec<(String, String)>,
    pub outbox: Vec<OutboxRow>,
}

#[derive(Debug, PartialEq)]
pub enum DbError {
    CommitFailed,
    MarkFailed,
}

#[derive(Default)]
pub struct Db {
    tables: Mutex<Tables>,
    commits: AtomicUsize,
    fail_commit: Mutex<Option<usize>>,
    failing_marks: Mutex<u32>,
}

impl Db {
    pub fn new() -> Db {
        Db::default()
    }

    pub fn fail_commit_number(&self, number: usize) {
        *self.fail_commit.lock().unwrap() = Some(number);
    }

    pub fn fail_next_marks(&self, count: u32) {
        *self.failing_marks.lock().unwrap() = count;
    }

    pub fn snapshot(&self) -> Tables {
        self.tables.lock().unwrap().clone()
    }

    pub fn transaction<R, E: From<DbError>>(&self, work: impl FnOnce(&mut Tables) -> Result<R, E>) -> Result<R, E> {
        let mut tables = self.tables.lock().unwrap();
        let mut staged = tables.clone();
        let result = work(&mut staged)?;
        let number = self.commits.fetch_add(1, Ordering::SeqCst) + 1;
        if *self.fail_commit.lock().unwrap() == Some(number) {
            return Err(E::from(DbError::CommitFailed));
        }
        *tables = staged;
        Ok(result)
    }

    pub fn pending(&self, limit: usize) -> Vec<OutboxRow> {
        self.tables.lock().unwrap().outbox.iter().filter(|row| !row.sent).take(limit).cloned().collect()
    }

    pub fn mark_sent(&self, seq: u64) -> Result<(), DbError> {
        let mut failing = self.failing_marks.lock().unwrap();
        if *failing > 0 {
            *failing -= 1;
            return Err(DbError::MarkFailed);
        }
        if let Some(row) = self.tables.lock().unwrap().outbox.iter_mut().find(|row| row.seq == seq) {
            row.sent = true;
        }
        Ok(())
    }
}
