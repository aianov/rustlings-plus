use contracts::UserId;
use scylla::Session;
use scylla::frame::value::CqlTimeuuid;
use scylla::prepared_statement::PreparedStatement;
use scylla::statement::Consistency;
use store_engine::{StoreError, prepared, timeuuid};
use uuid::Uuid;

use super::scylla::{ScyllaChats, big, bucket_of, chat_of, small};
use super::{BoxFuture, ChatExtras, Edited};

const LOOKUP: &str = "SELECT chat, bucket, author FROM messages_by_id WHERE id = ?";
const EDIT: &str = "UPDATE messages_by_chat USING TIMESTAMP ? SET body = ?, edited_at = ? WHERE chat = ? AND bucket = ? AND id = ?";
const MARK_READ: &str = "INSERT INTO chat_reads (user_id, peer, last_read) VALUES (?, ?, ?)";
const LAST_READ: &str = "SELECT last_read FROM chat_reads WHERE user_id = ? AND peer = ?";
const NEWER: &str =
    "SELECT author FROM messages_by_chat WHERE chat = ? AND bucket = ? AND id > ? LIMIT ?";

pub const UNREAD_CAP: u32 = 100;

pub struct More {
    lookup: PreparedStatement,
    edit: PreparedStatement,
    mark_read: PreparedStatement,
    last_read: PreparedStatement,
    newer: PreparedStatement,
}

impl More {
    pub async fn prepare(session: &Session) -> Result<More, StoreError> {
        let mut lookup = prepared(session, LOOKUP, true).await?;
        lookup.set_consistency(Consistency::LocalQuorum);
        let mut newer = prepared(session, NEWER, true).await?;
        newer.set_consistency(Consistency::LocalOne);
        Ok(More {
            lookup,
            edit: prepared(session, EDIT, true).await?,
            mark_read: prepared(session, MARK_READ, true).await?,
            last_read: prepared(session, LAST_READ, true).await?,
            newer,
        })
    }
}

fn other(error: impl std::fmt::Display) -> StoreError {
    StoreError::Other(error.to_string())
}

impl ScyllaChats {
    pub(super) async fn ensure_chat(
        &self,
        chat: &str,
        a: UserId,
        b: UserId,
        at: u64,
    ) -> Result<(), StoreError> {
        if self.opened.lock().await.contains(chat) {
            return Ok(());
        }
        self.open_chat(chat, a, b, at).await?;
        let members: Vec<i64> = vec![big(a.0), big(b.0)];
        self.session
            .execute(&self.st.add_members, (members, chat))
            .await?;
        self.opened.lock().await.insert(chat.to_string());
        Ok(())
    }

    pub async fn open_chat(
        &self,
        chat: &str,
        a: UserId,
        b: UserId,
        at: u64,
    ) -> Result<bool, StoreError> {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        let result = self
            .session
            .execute(
                &self.st.open_chat,
                (chat, big(first.0), big(second.0), big(at)),
            )
            .await?;
        let applied = result
            .rows
            .and_then(|rows| rows.into_iter().next())
            .and_then(|row| row.columns.into_iter().next().flatten())
            .and_then(|value| value.as_boolean());
        Ok(applied.unwrap_or(false))
    }

    pub async fn edit_at(
        &self,
        user: UserId,
        id: &str,
        text: &str,
        at: u64,
    ) -> Result<Edited, StoreError> {
        let Ok(uuid) = Uuid::parse_str(id) else {
            return Ok(Edited::Missing);
        };
        let id = CqlTimeuuid::from(uuid);
        let found = self
            .session
            .execute(&self.more.lookup, (id,))
            .await?
            .maybe_first_row_typed::<(String, i32, i64)>()
            .map_err(other)?;
        let Some((chat, bucket, author)) = found else {
            return Ok(Edited::Missing);
        };
        if small(author) != user.0 {
            return Ok(Edited::NotAuthor);
        }
        self.session
            .execute(
                &self.more.edit,
                (big(at), text, big(at), chat.as_str(), bucket, id),
            )
            .await?;
        Ok(Edited::Changed)
    }

    pub async fn unread_of(&self, user: UserId, peer: UserId) -> Result<u32, StoreError> {
        let last: Option<(CqlTimeuuid,)> = self
            .session
            .execute(&self.more.last_read, (big(user.0), big(peer.0)))
            .await?
            .maybe_first_row_typed()
            .map_err(other)?;
        let since = last.map_or_else(|| CqlTimeuuid::from(timeuuid(0, 0)), |(id,)| id);
        let chat = chat_of(user, peer);
        let newest = bucket_of(crate::common::now_micros());
        let mut unread = 0;
        for bucket in [newest, newest - 1] {
            let rows = self
                .session
                .execute(
                    &self.more.newer,
                    (
                        chat.as_str(),
                        bucket,
                        since,
                        i32::try_from(UNREAD_CAP).unwrap_or(i32::MAX),
                    ),
                )
                .await?
                .rows_typed_or_empty::<(i64,)>();
            for row in rows {
                let (author,) = row.map_err(other)?;
                if small(author) != user.0 {
                    unread += 1;
                }
            }
            if unread >= UNREAD_CAP {
                return Ok(UNREAD_CAP);
            }
        }
        Ok(unread)
    }
}

impl ChatExtras for ScyllaChats {
    fn send_for(
        &self,
        message: crate::common::Message,
        ttl_seconds: u32,
    ) -> BoxFuture<'_, Result<crate::common::Message, StoreError>> {
        Box::pin(async move {
            let at = crate::common::now_micros();
            let ttl = i32::try_from(ttl_seconds).unwrap_or(i32::MAX);
            self.append_for(&message, at, ttl).await
        })
    }

    fn edit(
        &self,
        user: UserId,
        id: String,
        text: String,
    ) -> BoxFuture<'_, Result<Edited, StoreError>> {
        Box::pin(async move {
            let at = crate::common::now_micros();
            self.edit_at(user, &id, &text, at).await
        })
    }

    fn mark_read(&self, user: UserId, peer: UserId) -> BoxFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let now = crate::common::now_micros();
            let mark = CqlTimeuuid::from(timeuuid(now, u64::MAX));
            self.session
                .execute(&self.more.mark_read, (big(user.0), big(peer.0), mark))
                .await?;
            Ok(())
        })
    }

    fn unread(&self, user: UserId, peer: UserId) -> BoxFuture<'_, Result<u32, StoreError>> {
        Box::pin(async move { self.unread_of(user, peer).await })
    }
}
