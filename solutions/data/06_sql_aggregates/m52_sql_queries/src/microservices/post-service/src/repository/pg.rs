use std::io;
use std::sync::Arc;

use contracts::{
    Cause, CommentId, Fact, LinkView, PostCommented, PostCreated, PostId, PostLiked, UserId,
};
use db_engine::{DbError, Migration, OUTBOX_SQL, record_fact};
use journal::JournalError;
use service_runtime::SortableIds;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::Notify;

use crate::common::{Draft, Liked, Link, Post};

pub const MIGRATIONS: [Migration; 2] = [
    ("0000_outbox", OUTBOX_SQL),
    (
        "0001_init.sql",
        include_str!("../../migrations/0001_init.sql"),
    ),
];

const INSERT_POST: &str =
    "INSERT INTO posts (id, author, author_name, text, at) VALUES ($1, $2, $3, $4, $5)";
const INSERT_TAG: &str = "INSERT INTO post_tags (post, position, tag) VALUES ($1, $2, $3)";
const INSERT_LINK: &str =
    "INSERT INTO post_links (post, position, url, title) VALUES ($1, $2, $3, $4)";
const SELECT_POST: &str = "SELECT id, author, author_name, text, likes, at, deleted FROM posts WHERE id = $1 AND NOT deleted";
const SELECT_TAGS: &str = "SELECT tag FROM post_tags WHERE post = $1 ORDER BY position";
const SELECT_LINKS: &str = "SELECT url, title FROM post_links WHERE post = $1 ORDER BY position";
pub(super) const SELECT_BY_AUTHORS: &str = "SELECT id, author, author_name, text, likes, at, deleted FROM posts WHERE author = ANY($1) AND NOT deleted ORDER BY id DESC OFFSET $2 LIMIT $3";
const INSERT_LIKE: &str =
    "INSERT INTO likes (post, user_id, at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING";
const COUNT_LIKE: &str = "UPDATE posts SET likes = likes + 1 WHERE id = $1";
pub(super) const EDIT_POST: &str = "UPDATE posts SET text = $2 WHERE id = $1 AND NOT deleted";
pub(super) const DELETE_POST: &str =
    "UPDATE posts SET deleted = true WHERE id = $1 AND NOT deleted";
const INSERT_COMMENT: &str =
    "INSERT INTO comments (post, author, text) VALUES ($1, $2, $3) RETURNING id";
pub(super) const LIKES_SINCE: &str = "SELECT l.post, count(*) FROM likes l JOIN posts p ON p.id = l.post WHERE l.at >= $1 AND NOT p.deleted GROUP BY l.post";
const NEWEST: &str = "SELECT max(id) FROM posts";

#[derive(sqlx::FromRow)]
pub(super) struct PostRow {
    pub(super) id: i64,
    pub(super) author: i64,
    pub(super) author_name: String,
    pub(super) text: String,
    pub(super) likes: i32,
    pub(super) at: i64,
    pub(super) deleted: bool,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    url: String,
    title: Option<String>,
}

pub(super) fn storage(error: impl std::fmt::Display) -> JournalError {
    JournalError::Io(io::Error::other(error.to_string()))
}

pub(super) fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn small(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub struct PgPosts {
    pub(super) pool: PgPool,
    pub(super) signal: Arc<Notify>,
    ids: SortableIds,
}

impl PgPosts {
    pub async fn open(pool: PgPool, node: u16) -> Result<PgPosts, DbError> {
        let newest: Option<i64> = sqlx::query_scalar(NEWEST).fetch_one(&pool).await?;
        let ids = SortableIds::new(node);
        if let Some(newest) = newest {
            ids.observe(small(newest));
        }
        Ok(PgPosts {
            pool,
            signal: Arc::default(),
            ids,
        })
    }

    pub(super) async fn commit(
        &self,
        mut tx: Transaction<'_, Postgres>,
        fact: Option<Fact>,
        cause: &Cause,
    ) -> Result<(), DbError> {
        if let Some(fact) = fact {
            record_fact(&mut tx, &fact, cause).await?;
        }
        tx.commit().await?;
        self.signal.notify_one();
        Ok(())
    }

    pub(super) async fn full(&self, row: PostRow) -> Result<Post, DbError> {
        let tags: Vec<String> = sqlx::query_scalar(SELECT_TAGS)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await?;
        let links: Vec<LinkRow> = sqlx::query_as(SELECT_LINKS)
            .bind(row.id)
            .fetch_all(&self.pool)
            .await?;
        Ok(Post {
            id: PostId(small(row.id)),
            author: UserId(small(row.author)),
            author_name: row.author_name,
            text: row.text,
            tags,
            links: links
                .into_iter()
                .map(|link| Link {
                    url: link.url,
                    title: link.title,
                })
                .collect(),
            likes: usize::try_from(row.likes).unwrap_or(0),
            at: small(row.at),
            deleted: row.deleted,
        })
    }

    pub(super) async fn post(&self, id: PostId) -> Result<Option<Post>, DbError> {
        let row: Option<PostRow> = sqlx::query_as(SELECT_POST)
            .bind(big(id.0))
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(self.full(row).await?)),
            None => Ok(None),
        }
    }

    pub(super) async fn create(
        &self,
        author: UserId,
        author_name: String,
        draft: Draft,
        cause: &Cause,
    ) -> Result<Post, DbError> {
        let id = self.ids.next();
        let mut tx = self.pool.begin().await?;
        sqlx::query(INSERT_POST)
            .bind(big(id))
            .bind(big(author.0))
            .bind(&author_name)
            .bind(draft.text.as_ref())
            .bind(big(cause.at))
            .execute(&mut *tx)
            .await?;
        let tags: Vec<String> = draft
            .tags
            .iter()
            .map(|tag| tag.as_ref().to_string())
            .collect();
        for (position, tag) in tags.iter().enumerate() {
            sqlx::query(INSERT_TAG)
                .bind(big(id))
                .bind(i32::try_from(position).unwrap_or(i32::MAX))
                .bind(tag)
                .execute(&mut *tx)
                .await?;
        }
        for (position, link) in draft.links.iter().enumerate() {
            sqlx::query(INSERT_LINK)
                .bind(big(id))
                .bind(i32::try_from(position).unwrap_or(i32::MAX))
                .bind(&link.url)
                .bind(&link.title)
                .execute(&mut *tx)
                .await?;
        }
        let post = Post {
            id: PostId(id),
            author,
            author_name: author_name.clone(),
            text: draft.text.as_ref().to_string(),
            tags: tags.clone(),
            links: draft.links.clone(),
            likes: 0,
            at: cause.at,
            deleted: false,
        };
        let fact = Fact::PostCreated(PostCreated {
            post_id: post.id,
            author_id: author,
            author_name,
            text: post.text.clone(),
            tags,
            links: post
                .links
                .iter()
                .map(|link| LinkView {
                    url: link.url.clone(),
                    title: link.title.clone(),
                })
                .collect(),
        });
        self.commit(tx, Some(fact), cause).await?;
        Ok(post)
    }

    pub(super) async fn like(
        &self,
        user: UserId,
        post: PostId,
        cause: &Cause,
    ) -> Result<Option<Liked>, DbError> {
        let Some(found) = self.post(post).await? else {
            return Ok(None);
        };
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(INSERT_LIKE)
            .bind(big(post.0))
            .bind(big(user.0))
            .bind(big(cause.at))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if changed {
            sqlx::query(COUNT_LIKE)
                .bind(big(post.0))
                .execute(&mut *tx)
                .await?;
        }
        let fact = Fact::PostLiked(PostLiked {
            post_id: post,
            author_id: found.author,
            user_id: user,
        });
        self.commit(tx, changed.then_some(fact), cause).await?;
        Ok(Some(Liked {
            changed,
            author: found.author,
        }))
    }

    pub(super) async fn change(
        &self,
        query: &str,
        post: PostId,
        text: Option<&str>,
        fact: Fact,
        cause: &Cause,
    ) -> Result<bool, DbError> {
        let mut tx = self.pool.begin().await?;
        let mut statement = sqlx::query(query).bind(big(post.0));
        if let Some(text) = text {
            statement = statement.bind(text.to_string());
        }
        let changed = statement.execute(&mut *tx).await?.rows_affected() == 1;
        self.commit(tx, changed.then_some(fact), cause).await?;
        Ok(changed)
    }

    pub(super) async fn reply(
        &self,
        author: UserId,
        post: PostId,
        text: &str,
        cause: &Cause,
    ) -> Result<Option<CommentId>, DbError> {
        let Some(found) = self.post(post).await? else {
            return Ok(None);
        };
        let mut tx = self.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(INSERT_COMMENT)
            .bind(big(post.0))
            .bind(big(author.0))
            .bind(text)
            .fetch_one(&mut *tx)
            .await?;
        let comment = CommentId(small(id));
        let fact = Fact::PostCommented(PostCommented {
            post_id: post,
            author_id: found.author,
            comment_id: comment,
            commenter_id: author,
        });
        self.commit(tx, Some(fact), cause).await?;
        Ok(Some(comment))
    }
}
