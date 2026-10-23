use std::sync::Arc;

use contracts::{PostId, UserId};
use resp_client::{Client, cmd, key};
use scylla::Session;
use scylla::prepared_statement::PreparedStatement;
use store_engine::{Schema, StoreError, prepared};

use super::memory::Snapshot;
use super::{BoxFuture, FeedRepository};
use crate::common::{Change, Consumed, Cursor, FeedItem};

pub const SCHEMA: [Schema; 1] = [("0001_feed.cql", include_str!("../../cql/0001_feed.cql"))];

pub const CACHED: i64 = 200;

const APPLIED: &str = "SELECT version FROM feed_applied WHERE aggregate = ?";
const MARK: &str = "INSERT INTO feed_applied (aggregate, version) VALUES (?, ?)";
const NAME: &str = "INSERT INTO feed_names (user_id, name) VALUES (?, ?) USING TIMESTAMP ?";
const FOLLOW: &str = "INSERT INTO followers_of (author, follower) VALUES (?, ?) USING TIMESTAMP ?";
const MUTE: &str = "INSERT INTO feed_mutes (user_id, muted) VALUES (?, ?) USING TIMESTAMP ?";
const POST: &str = "INSERT INTO feed_posts (post, author, author_name, body, likes, tags, deleted, at) VALUES (?, ?, ?, ?, 0, ?, false, ?) USING TIMESTAMP ?";
const FOLLOWERS: &str = "SELECT follower FROM followers_of WHERE author = ?";
const DELIVER: &str =
    "INSERT INTO feed_by_user (user_id, at, post) VALUES (?, ?, ?) USING TIMESTAMP ?";
const LIKES: &str = "SELECT likes FROM feed_posts WHERE post = ?";
const SET_LIKES: &str = "UPDATE feed_posts USING TIMESTAMP ? SET likes = ? WHERE post = ?";
const EDIT: &str = "UPDATE feed_posts USING TIMESTAMP ? SET body = ? WHERE post = ?";
const DELETE: &str = "UPDATE feed_posts USING TIMESTAMP ? SET deleted = true WHERE post = ?";
const PAGE: &str =
    "SELECT at, post FROM feed_by_user WHERE user_id = ? AND (at, post) < (?, ?) LIMIT ?";
const DETAILS: &str = "SELECT post, author, author_name, body, likes, tags, deleted, at FROM feed_posts WHERE post = ?";
const MUTES: &str = "SELECT muted FROM feed_mutes WHERE user_id = ?";

pub(super) struct Statements {
    applied: PreparedStatement,
    mark: PreparedStatement,
    name: PreparedStatement,
    follow: PreparedStatement,
    mute: PreparedStatement,
    post: PreparedStatement,
    followers: PreparedStatement,
    deliver: PreparedStatement,
    likes: PreparedStatement,
    set_likes: PreparedStatement,
    edit: PreparedStatement,
    delete: PreparedStatement,
    pub(super) page: PreparedStatement,
    pub(super) details: PreparedStatement,
    pub(super) mutes: PreparedStatement,
}

pub struct ScyllaFeed {
    pub(super) session: Arc<Session>,
    pub(super) st: Statements,
    pub(super) cache: Option<Client>,
}

pub(super) type Details = (
    i64,
    i64,
    String,
    String,
    i64,
    Option<Vec<String>>,
    bool,
    i64,
);

pub(super) fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn small(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub(super) fn feed_key(user: UserId) -> String {
    key("feed", "user", user.0)
}

impl ScyllaFeed {
    pub async fn open(
        session: Arc<Session>,
        cache: Option<Client>,
    ) -> Result<ScyllaFeed, StoreError> {
        store_engine::apply(&session, &SCHEMA).await?;
        let st = Statements {
            applied: prepared(&session, APPLIED, true).await?,
            mark: prepared(&session, MARK, true).await?,
            name: prepared(&session, NAME, true).await?,
            follow: prepared(&session, FOLLOW, true).await?,
            mute: prepared(&session, MUTE, true).await?,
            post: prepared(&session, POST, true).await?,
            followers: prepared(&session, FOLLOWERS, true).await?,
            deliver: prepared(&session, DELIVER, true).await?,
            likes: prepared(&session, LIKES, true).await?,
            set_likes: prepared(&session, SET_LIKES, false).await?,
            edit: prepared(&session, EDIT, true).await?,
            delete: prepared(&session, DELETE, true).await?,
            page: prepared(&session, PAGE, true).await?,
            details: prepared(&session, DETAILS, true).await?,
            mutes: prepared(&session, MUTES, true).await?,
        };
        Ok(ScyllaFeed { session, st, cache })
    }

    async fn seen(&self, aggregate: &str) -> Result<u64, StoreError> {
        let row = self
            .session
            .execute(&self.st.applied, (aggregate,))
            .await?
            .maybe_first_row_typed::<(i64,)>()
            .map_err(|error| StoreError::Other(error.to_string()))?;
        Ok(row.map_or(0, |(version,)| small(version)))
    }

    async fn followers(&self, author: UserId) -> Result<Vec<UserId>, StoreError> {
        let rows = self
            .session
            .execute(&self.st.followers, (big(author.0),))
            .await?
            .rows_typed_or_empty::<(i64,)>();
        let mut found = vec![author];
        for row in rows {
            let (follower,) = row.map_err(|error| StoreError::Other(error.to_string()))?;
            found.push(UserId(small(follower)));
        }
        Ok(found)
    }

    async fn deliver(
        &self,
        post: PostId,
        author: UserId,
        at: u64,
        stamp: i64,
    ) -> Result<(), StoreError> {
        for reader in self.followers(author).await? {
            self.session
                .execute(
                    &self.st.deliver,
                    (big(reader.0), big(at), big(post.0), stamp),
                )
                .await?;
            if let Some(cache) = &self.cache {
                let key = feed_key(reader);
                let entry = format!("{at}:{}", post.0);
                let cached = cache
                    .pipeline(vec![
                        cmd("LPUSH").arg(&key).arg(entry),
                        cmd("LTRIM").arg(&key).arg(0).arg(CACHED - 1),
                        cmd("EXPIRE").arg(&key).arg(24 * 60 * 60),
                    ])
                    .await;
                if let Err(error) = cached {
                    tracing::warn!("feed-service: кэш ленты не обновлён: {error}");
                }
            }
        }
        Ok(())
    }

    async fn store(
        &self,
        aggregate: &str,
        version: u64,
        change: Change,
    ) -> Result<Consumed, StoreError> {
        if version <= self.seen(aggregate).await? {
            return Ok(Consumed::Duplicate);
        }
        let stamp = big(version);
        match change {
            Change::Registered { user, username } => {
                self.session
                    .execute(&self.st.name, (big(user.0), username, stamp))
                    .await?;
            }
            Change::Renamed { user, display_name } => {
                if let Some(name) = display_name {
                    self.session
                        .execute(&self.st.name, (big(user.0), name, stamp))
                        .await?;
                }
            }
            Change::Followed { follower, followee } => {
                self.session
                    .execute(&self.st.follow, (big(followee.0), big(follower.0), stamp))
                    .await?;
            }
            Change::Muted { user, muted } => {
                self.session
                    .execute(&self.st.mute, (big(user.0), big(muted.0), stamp))
                    .await?;
            }
            Change::Posted {
                post,
                author,
                author_name,
                text,
                tags,
                at,
                ..
            } => {
                let values = (
                    big(post.0),
                    big(author.0),
                    author_name,
                    text,
                    tags,
                    big(at),
                    stamp,
                );
                self.session.execute(&self.st.post, values).await?;
                self.deliver(post, author, at, stamp).await?;
            }
            Change::Liked { post } => {
                let likes = self
                    .session
                    .execute(&self.st.likes, (big(post.0),))
                    .await?
                    .maybe_first_row_typed::<(Option<i64>,)>()
                    .map_err(|error| StoreError::Other(error.to_string()))?;
                let likes = likes.and_then(|(likes,)| likes).unwrap_or(0) + 1;
                self.session
                    .execute(&self.st.set_likes, (stamp, likes, big(post.0)))
                    .await?;
            }
            Change::Edited { post, text } => {
                self.session
                    .execute(&self.st.edit, (stamp, text, big(post.0)))
                    .await?;
            }
            Change::Deleted { post } => {
                self.session
                    .execute(&self.st.delete, (stamp, big(post.0)))
                    .await?;
            }
            Change::Gone { .. } | Change::Nothing => {}
        }
        self.session
            .execute(&self.st.mark, (aggregate, stamp))
            .await?;
        Ok(Consumed::Applied)
    }
}

impl FeedRepository for ScyllaFeed {
    fn apply(&self, aggregate: String, version: u64, change: Change) -> BoxFuture<'_, Consumed> {
        Box::pin(async move {
            match self.store(&aggregate, version, change).await {
                Ok(consumed) => consumed,
                Err(error) => {
                    tracing::warn!(
                        "feed-service: событие {aggregate}#{version} не легло в Scylla: {error}"
                    );
                    Consumed::Buffered
                }
            }
        })
    }

    fn page(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<FeedItem>> {
        Box::pin(async move {
            let found = self
                .page_after(viewer, None, skip.saturating_add(take))
                .await;
            found
                .map(|(items, _)| items.into_iter().skip(skip).collect())
                .unwrap_or_default()
        })
    }

    fn after(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> BoxFuture<'_, (Vec<FeedItem>, Option<Cursor>)> {
        Box::pin(async move {
            self.page_after(viewer, before, take)
                .await
                .unwrap_or_default()
        })
    }

    fn drain(&self, _budget: usize) -> BoxFuture<'_, usize> {
        Box::pin(async { 0 })
    }

    fn pending(&self) -> BoxFuture<'_, usize> {
        Box::pin(async { 0 })
    }

    fn snapshot(&self) -> BoxFuture<'_, Snapshot> {
        Box::pin(async { Snapshot::default() })
    }
}
