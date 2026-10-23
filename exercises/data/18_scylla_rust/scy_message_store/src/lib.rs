#![allow(unused_variables, dead_code, unused_imports)]

use std::fmt::{self, Write};
use std::sync::Arc;

use bytes::Bytes;
use futures::future::try_join_all;
use scylla::prepared_statement::PreparedStatement;
use scylla::statement::Consistency;
use scylla::transport::errors::{DbError, QueryError};
use scylla::Session;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: Uuid,
    pub sent_at: i64,
    pub author: i32,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub items: Vec<Message>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatSummary {
    pub chat: i32,
    pub last_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    BadCursor,
    Invalid(String),
    Unavailable,
    Timeout,
    Other(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::BadCursor => f.write_str("курсор испорчен"),
            StoreError::Invalid(msg) => write!(f, "неверный запрос: {msg}"),
            StoreError::Unavailable => f.write_str("хранилище недоступно"),
            StoreError::Timeout => f.write_str("хранилище не ответило в срок"),
            StoreError::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<QueryError> for StoreError {
    fn from(e: QueryError) -> StoreError {
        match e {
            QueryError::DbError(DbError::Invalid | DbError::SyntaxError, msg) => StoreError::Invalid(msg),
            QueryError::DbError(DbError::Unavailable { .. } | DbError::Overloaded | DbError::IsBootstrapping, _)
            | QueryError::IoError(_) => StoreError::Unavailable,
            QueryError::DbError(DbError::ReadTimeout { .. } | DbError::WriteTimeout { .. }, _)
            | QueryError::TimeoutError
            | QueryError::RequestTimeout(_) => StoreError::Timeout,
            other => StoreError::Other(other.to_string()),
        }
    }
}

fn other(e: impl fmt::Display) -> StoreError {
    StoreError::Other(e.to_string())
}

pub fn encode_cursor(state: &[u8]) -> String {
    let mut out = String::with_capacity(state.len() * 2);
    for b in state {
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn decode_cursor(cursor: &str) -> Option<Bytes> {
    if cursor.is_empty() || cursor.len() % 2 != 0 {
        return None;
    }
    (0..cursor.len())
        .step_by(2)
        .map(|i| cursor.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect::<Option<Vec<u8>>>()
        .map(Bytes::from)
}

pub struct MessageStore {
    session: Arc<Session>,
}

impl MessageStore {
    pub async fn new(session: Arc<Session>, keyspace: &str) -> Result<MessageStore, StoreError> {
        todo!("таблицы, согласование схемы, подготовка запросов")
    }

    pub async fn send(&self, chat: i32, author: i32, body: &str, members: &[i32], at_ms: i64) -> Result<Uuid, StoreError> {
        todo!("сообщение в messages и активность всех участников и автора")
    }

    pub async fn history(&self, chat: i32, cursor: Option<&str>, limit: i32) -> Result<Page, StoreError> {
        todo!("страница истории и курсор следующей")
    }

    pub async fn chats(&self, user: i32) -> Result<Vec<ChatSummary>, StoreError> {
        todo!("чаты пользователя по последней активности")
    }
}
