use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use contracts::{Cause, Fact, MessageSent, Outbox, UserId};
use journal::JournalError;
use scylla::Session;
use scylla::batch::{Batch, BatchType};
use scylla::frame::value::CqlTimeuuid;
use scylla::prepared_statement::PreparedStatement;
use scylla::statement::Consistency;
use store_engine::{Cursor, Schema, StoreError, prepared, timeuuid};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use super::scylla_more::More;
use super::{BoxFuture, ChatRepository, Page};
use crate::common::{Message, now_micros};

pub const SCHEMA: [Schema; 2] = [
    ("0001_chat.cql", include_str!("../../cql/0001_chat.cql")),
    (
        "0002_chat_more.cql",
        include_str!("../../cql/0002_chat_more.cql"),
    ),
];

pub const BUCKETS_BACK: i32 = 24;

pub const PREVIEW_CHARS: usize = 60;

const INSERT_MESSAGE: &str = "INSERT INTO messages_by_chat (chat, bucket, id, author, peer, body) VALUES (?, ?, ?, ?, ?, ?) USING TTL ? AND TIMESTAMP ?";
const INSERT_BY_ID: &str = "INSERT INTO messages_by_id (id, chat, bucket, author) VALUES (?, ?, ?, ?) USING TTL ? AND TIMESTAMP ?";
const OPEN_CHAT: &str =
    "INSERT INTO chats (chat, first, second, created_at) VALUES (?, ?, ?, ?) IF NOT EXISTS";
const ADD_MEMBERS: &str = "UPDATE chats SET members = members + ? WHERE chat = ?";
const HISTORY: &str =
    "SELECT id, author, peer, body, edited_at FROM messages_by_chat WHERE chat = ? AND bucket = ?";
const LAST: &str = "SELECT last_at FROM chat_activity WHERE user_id = ? AND peer = ?";
const REMOVE: &str = "DELETE FROM chats_by_user WHERE user_id = ? AND last_at = ? AND peer = ?";
const ADD: &str = "INSERT INTO chats_by_user (user_id, last_at, peer, preview) VALUES (?, ?, ?, ?)";
const SET: &str = "INSERT INTO chat_activity (user_id, peer, last_at) VALUES (?, ?, ?)";
const CHATS: &str = "SELECT peer, last_at, preview FROM chats_by_user WHERE user_id = ? LIMIT ?";

pub(super) struct Statements {
    insert_message: PreparedStatement,
    insert_by_id: PreparedStatement,
    pub(super) open_chat: PreparedStatement,
    pub(super) add_members: PreparedStatement,
    pub(super) history: PreparedStatement,
    last: PreparedStatement,
    remove: PreparedStatement,
    add: PreparedStatement,
    set: PreparedStatement,
    pub(super) chats: PreparedStatement,
}

pub struct ScyllaChats {
    pub(super) session: Arc<Session>,
    pub(super) st: Statements,
    pub(super) outbox: Mutex<Outbox>,
    pub(super) signal: Arc<Notify>,
    salt: AtomicU64,
    pub(super) opened: Mutex<HashSet<String>>,
    pub(super) more: More,
}

pub(super) type MessageRow = (CqlTimeuuid, i64, i64, String, Option<i64>);

pub(super) fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn small(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub fn chat_of(a: UserId, b: UserId) -> String {
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    format!("{}:{}", low.0, high.0)
}

pub fn bucket_of(micros: u64) -> i32 {
    let days = i64::try_from(micros / 86_400_000_000).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    i32::try_from(year * 12 + month - 1).unwrap_or(0)
}

fn read(mut statement: PreparedStatement) -> PreparedStatement {
    statement.set_consistency(Consistency::LocalOne);
    statement
}

pub(super) fn id_text(id: &CqlTimeuuid) -> String {
    Uuid::from_bytes(*id.as_bytes()).to_string()
}

pub(super) fn message_of((id, author, peer, body, edited): MessageRow) -> Message {
    Message {
        from: UserId(small(author)),
        to: UserId(small(peer)),
        text: body,
        id: id_text(&id),
        edited: edited.is_some(),
    }
}

impl ScyllaChats {
    pub async fn open(session: Arc<Session>) -> Result<ScyllaChats, StoreError> {
        store_engine::apply(&session, &SCHEMA).await?;
        let st = Statements {
            insert_message: prepared(&session, INSERT_MESSAGE, true).await?,
            insert_by_id: prepared(&session, INSERT_BY_ID, true).await?,
            open_chat: prepared(&session, OPEN_CHAT, false).await?,
            add_members: prepared(&session, ADD_MEMBERS, false).await?,
            history: read(prepared(&session, HISTORY, true).await?),
            last: prepared(&session, LAST, true).await?,
            remove: prepared(&session, REMOVE, true).await?,
            add: prepared(&session, ADD, true).await?,
            set: prepared(&session, SET, true).await?,
            chats: read(prepared(&session, CHATS, true).await?),
        };
        let more = More::prepare(&session).await?;
        Ok(ScyllaChats {
            session,
            st,
            outbox: Mutex::new(Outbox::default()),
            signal: Arc::default(),
            salt: AtomicU64::new(0),
            opened: Mutex::new(HashSet::new()),
            more,
        })
    }

    async fn touch(
        &self,
        user: UserId,
        peer: UserId,
        at: i64,
        preview: &str,
    ) -> Result<(), StoreError> {
        let last = self
            .session
            .execute(&self.st.last, (big(user.0), big(peer.0)))
            .await?
            .maybe_first_row_typed::<(i64,)>()
            .map_err(|error| StoreError::Other(error.to_string()))?;
        if let Some((old,)) = last {
            self.session
                .execute(&self.st.remove, (big(user.0), old, big(peer.0)))
                .await?;
        }
        self.session
            .execute(&self.st.add, (big(user.0), at, big(peer.0), preview))
            .await?;
        self.session
            .execute(&self.st.set, (big(user.0), big(peer.0), at))
            .await?;
        Ok(())
    }

    pub async fn append_at(&self, message: &Message, at: u64) -> Result<Message, StoreError> {
        self.append_for(message, at, 0).await
    }

    pub async fn append_for(
        &self,
        message: &Message,
        at: u64,
        ttl: i32,
    ) -> Result<Message, StoreError> {
        let chat = chat_of(message.from, message.to);
        self.ensure_chat(&chat, message.from, message.to, at)
            .await?;
        let salt = self.salt.fetch_add(1, Ordering::Relaxed);
        let id = CqlTimeuuid::from(timeuuid(at, salt));
        let bucket = bucket_of(at);
        let mut both = Batch::new(BatchType::Logged);
        both.append_statement(self.st.insert_message.clone());
        both.append_statement(self.st.insert_by_id.clone());
        let values = (
            (
                chat.as_str(),
                bucket,
                id,
                big(message.from.0),
                big(message.to.0),
                message.text.as_str(),
                ttl,
                big(at),
            ),
            (id, chat.as_str(), bucket, big(message.from.0), ttl, big(at)),
        );
        self.session.batch(&both, values).await?;
        let preview: String = message.text.chars().take(PREVIEW_CHARS).collect();
        let stamp = big(at);
        self.touch(message.from, message.to, stamp, &preview)
            .await?;
        self.touch(message.to, message.from, stamp, &preview)
            .await?;
        Ok(Message {
            id: id_text(&id),
            ..message.clone()
        })
    }

    pub(super) async fn page_from(
        &self,
        user: UserId,
        peer: UserId,
        cursor: Option<Cursor>,
        limit: usize,
    ) -> Result<Page, StoreError> {
        let chat = chat_of(user, peer);
        let newest = bucket_of(now_micros());
        let mut at = cursor.unwrap_or(Cursor {
            bucket: newest,
            state: None,
        });
        let mut found = Vec::with_capacity(limit);
        while found.len() < limit && at.bucket > newest - BUCKETS_BACK {
            let mut statement = self.st.history.clone();
            statement.set_page_size(i32::try_from(limit - found.len()).unwrap_or(i32::MAX));
            let result = self
                .session
                .execute_paged(&statement, (chat.as_str(), at.bucket), at.state.clone())
                .await?;
            let state = result.paging_state.clone();
            let rows = result.rows_typed_or_empty::<MessageRow>();
            for row in rows {
                found.push(message_of(
                    row.map_err(|error| StoreError::Other(error.to_string()))?,
                ));
            }
            at = match state {
                Some(state) => Cursor {
                    bucket: at.bucket,
                    state: Some(state),
                },
                None => Cursor {
                    bucket: at.bucket - 1,
                    state: None,
                },
            };
        }
        let next = (at.bucket > newest - BUCKETS_BACK).then(|| at.encode());
        Ok((found, next))
    }
}

impl ChatRepository for ScyllaChats {
    fn append(
        &self,
        message: Message,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Message, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let message = self
                .append_at(&message, cause.at)
                .await
                .map_err(|error| JournalError::Io(std::io::Error::other(error.to_string())))?;
            let fact = Fact::MessageSent(MessageSent {
                from_id: message.from,
                to_id: message.to,
                text: message.text.clone(),
            });
            let mut outbox = self.outbox.lock().await;
            let row = outbox.prepare(&fact, &cause);
            outbox.push(row);
            self.signal.notify_one();
            Ok(message)
        })
    }

    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>> {
        Box::pin(async move {
            let (mut messages, _) = self
                .page_from(user, peer, None, limit)
                .await
                .unwrap_or_default();
            messages.reverse();
            messages
        })
    }
}
