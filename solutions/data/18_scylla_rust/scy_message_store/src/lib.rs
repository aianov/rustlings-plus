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

struct Statements {
    insert_message: PreparedStatement,
    history: PreparedStatement,
    last_activity: PreparedStatement,
    remove_activity: PreparedStatement,
    add_activity: PreparedStatement,
    set_activity: PreparedStatement,
    chats: PreparedStatement,
}

pub struct MessageStore {
    session: Arc<Session>,
    st: Statements,
}

async fn prepare(session: &Session, text: String, idempotent: bool) -> Result<PreparedStatement, QueryError> {
    let mut stmt = session.prepare(text).await?;
    stmt.set_consistency(Consistency::Quorum);
    stmt.set_is_idempotent(idempotent);
    Ok(stmt)
}

impl MessageStore {
    pub async fn new(session: Arc<Session>, keyspace: &str) -> Result<MessageStore, StoreError> {
        let ks = keyspace;
        for ddl in [
            format!(
                "CREATE TABLE IF NOT EXISTS {ks}.messages (chat_id int, sent_at bigint, id uuid, author int, body text, \
                 PRIMARY KEY (chat_id, sent_at, id)) WITH CLUSTERING ORDER BY (sent_at DESC, id ASC)"
            ),
            format!(
                "CREATE TABLE IF NOT EXISTS {ks}.chats_by_user (user_id int, last_at bigint, chat_id int, \
                 PRIMARY KEY (user_id, last_at, chat_id)) WITH CLUSTERING ORDER BY (last_at DESC, chat_id ASC)"
            ),
            format!(
                "CREATE TABLE IF NOT EXISTS {ks}.chat_activity (user_id int, chat_id int, last_at bigint, PRIMARY KEY (user_id, chat_id))"
            ),
        ] {
            session.query(ddl, ()).await?;
        }
        session.await_schema_agreement().await?;
        let st = Statements {
            insert_message: prepare(
                &session,
                format!("INSERT INTO {ks}.messages (chat_id, sent_at, id, author, body) VALUES (?, ?, ?, ?, ?)"),
                true,
            )
            .await?,
            history: prepare(&session, format!("SELECT sent_at, id, author, body FROM {ks}.messages WHERE chat_id = ?"), true).await?,
            last_activity: prepare(
                &session,
                format!("SELECT last_at FROM {ks}.chat_activity WHERE user_id = ? AND chat_id = ?"),
                true,
            )
            .await?,
            remove_activity: prepare(
                &session,
                format!("DELETE FROM {ks}.chats_by_user WHERE user_id = ? AND last_at = ? AND chat_id = ?"),
                true,
            )
            .await?,
            add_activity: prepare(
                &session,
                format!("INSERT INTO {ks}.chats_by_user (user_id, last_at, chat_id) VALUES (?, ?, ?)"),
                true,
            )
            .await?,
            set_activity: prepare(
                &session,
                format!("INSERT INTO {ks}.chat_activity (user_id, chat_id, last_at) VALUES (?, ?, ?)"),
                true,
            )
            .await?,
            chats: prepare(&session, format!("SELECT chat_id, last_at FROM {ks}.chats_by_user WHERE user_id = ?"), true).await?,
        };
        Ok(MessageStore { session, st })
    }

    pub async fn send(&self, chat: i32, author: i32, body: &str, members: &[i32], at_ms: i64) -> Result<Uuid, StoreError> {
        let id = Uuid::new_v4();
        self.session.execute(&self.st.insert_message, (chat, at_ms, id, author, body)).await?;
        let mut users: Vec<i32> = members.to_vec();
        if !users.contains(&author) {
            users.push(author);
        }
        try_join_all(users.into_iter().map(|user| self.touch(user, chat, at_ms))).await?;
        Ok(id)
    }

    async fn touch(&self, user: i32, chat: i32, at_ms: i64) -> Result<(), StoreError> {
        let res = self.session.execute(&self.st.last_activity, (user, chat)).await?;
        let old = res.maybe_first_row_typed::<(i64,)>().map_err(other)?.map(|(t,)| t);
        if old.is_some_and(|old| old >= at_ms) {
            return Ok(());
        }
        if let Some(old) = old {
            self.session.execute(&self.st.remove_activity, (user, old, chat)).await?;
        }
        self.session.execute(&self.st.add_activity, (user, at_ms, chat)).await?;
        self.session.execute(&self.st.set_activity, (user, chat, at_ms)).await?;
        Ok(())
    }

    pub async fn history(&self, chat: i32, cursor: Option<&str>, limit: i32) -> Result<Page, StoreError> {
        let state = match cursor {
            Some(c) => Some(decode_cursor(c).ok_or(StoreError::BadCursor)?),
            None => None,
        };
        let mut stmt = self.st.history.clone();
        stmt.set_page_size(limit.max(1));
        let res = match self.session.execute_paged(&stmt, (chat,), state).await {
            Ok(res) => res,
            Err(QueryError::DbError(DbError::ProtocolError | DbError::Invalid, _)) if cursor.is_some() => {
                return Err(StoreError::BadCursor);
            }
            Err(e) => return Err(e.into()),
        };
        let next = res.paging_state.as_deref().map(encode_cursor);
        let items = res
            .rows_typed::<(i64, Uuid, i32, String)>()
            .map_err(other)?
            .map(|r| r.map(|(sent_at, id, author, body)| Message { id, sent_at, author, body }))
            .collect::<Result<Vec<_>, _>>()
            .map_err(other)?;
        Ok(Page { items, next })
    }

    pub async fn chats(&self, user: i32) -> Result<Vec<ChatSummary>, StoreError> {
        let res = self.session.execute(&self.st.chats, (user,)).await?;
        res.rows_typed::<(i32, i64)>()
            .map_err(other)?
            .map(|r| r.map(|(chat, last_at)| ChatSummary { chat, last_at }).map_err(other))
            .collect()
    }
}
