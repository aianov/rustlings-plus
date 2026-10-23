use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use contracts::{Cause, Fact, MessageSent, Outbox, UserId};
use journal::JournalError;
use scylla::Session;
use scylla::frame::value::CqlTimeuuid;
use scylla::prepared_statement::PreparedStatement;
use store_engine::{Cursor, Schema, StoreError, prepared, timeuuid};
use tokio::sync::{Mutex, Notify};

use super::{BoxFuture, ChatRepository, Page};
use crate::common::{Message, now_micros};

pub const SCHEMA: [Schema; 1] = [("0001_chat.cql", include_str!("../../cql/0001_chat.cql"))];

pub const BUCKETS_BACK: i32 = 24;

pub const PREVIEW_CHARS: usize = 60;

const INSERT_MESSAGE: &str =
    "INSERT INTO messages_by_chat (chat, bucket, id, author, peer, body) VALUES (?, ?, ?, ?, ?, ?)";
const HISTORY: &str =
    "SELECT id, author, peer, body FROM messages_by_chat WHERE chat = ? AND bucket = ?";
const LAST: &str = "SELECT last_at FROM chat_activity WHERE user_id = ? AND peer = ?";
const REMOVE: &str = "DELETE FROM chats_by_user WHERE user_id = ? AND last_at = ? AND peer = ?";
const ADD: &str = "INSERT INTO chats_by_user (user_id, last_at, peer, preview) VALUES (?, ?, ?, ?)";
const SET: &str = "INSERT INTO chat_activity (user_id, peer, last_at) VALUES (?, ?, ?)";
const MEMBER: &str = "INSERT INTO chat_members (chat, user_id) VALUES (?, ?)";
const CHATS: &str = "SELECT peer, last_at, preview FROM chats_by_user WHERE user_id = ? LIMIT ?";

pub(super) struct Statements {
    insert_message: PreparedStatement,
    pub(super) history: PreparedStatement,
    last: PreparedStatement,
    remove: PreparedStatement,
    add: PreparedStatement,
    set: PreparedStatement,
    member: PreparedStatement,
    pub(super) chats: PreparedStatement,
}

pub struct ScyllaChats {
    pub(super) session: Arc<Session>,
    pub(super) st: Statements,
    pub(super) outbox: Mutex<Outbox>,
    pub(super) signal: Arc<Notify>,
    salt: AtomicU64,
}

pub(super) type MessageRow = (CqlTimeuuid, i64, i64, String);

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

pub(super) fn message_of((_, author, peer, body): MessageRow) -> Message {
    Message {
        from: UserId(small(author)),
        to: UserId(small(peer)),
        text: body,
    }
}

impl ScyllaChats {
    pub async fn open(session: Arc<Session>) -> Result<ScyllaChats, StoreError> {
        store_engine::apply(&session, &SCHEMA).await?;
        let st = Statements {
            insert_message: prepared(&session, INSERT_MESSAGE, true).await?,
            history: prepared(&session, HISTORY, true).await?,
            last: prepared(&session, LAST, true).await?,
            remove: prepared(&session, REMOVE, true).await?,
            add: prepared(&session, ADD, true).await?,
            set: prepared(&session, SET, true).await?,
            member: prepared(&session, MEMBER, true).await?,
            chats: prepared(&session, CHATS, true).await?,
        };
        Ok(ScyllaChats {
            session,
            st,
            outbox: Mutex::new(Outbox::default()),
            signal: Arc::default(),
            salt: AtomicU64::new(0),
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

    pub async fn append_at(&self, message: &Message, at: u64) -> Result<(), StoreError> {
        let chat = chat_of(message.from, message.to);
        let salt = self.salt.fetch_add(1, Ordering::Relaxed);
        let id = CqlTimeuuid::from(timeuuid(at, salt));
        self.session
            .execute(
                &self.st.insert_message,
                (
                    chat.as_str(),
                    bucket_of(at),
                    id,
                    big(message.from.0),
                    big(message.to.0),
                    message.text.as_str(),
                ),
            )
            .await?;
        let preview: String = message.text.chars().take(PREVIEW_CHARS).collect();
        let at = big(at);
        self.touch(message.from, message.to, at, &preview).await?;
        self.touch(message.to, message.from, at, &preview).await?;
        for member in [message.from, message.to] {
            self.session
                .execute(&self.st.member, (chat.as_str(), big(member.0)))
                .await?;
        }
        Ok(())
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
            self.append_at(&message, cause.at)
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
