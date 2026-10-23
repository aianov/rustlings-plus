use contracts::{NoticeKind, PostId, UserId};
use db_engine::{DbError, Migration, OUTBOX_SQL};
use sqlx::PgPool;

use super::{BoxFuture, NoticeRepository, Recorded};
use crate::common::{Draft, Notice};

pub const MIGRATIONS: [Migration; 2] = [
    ("0000_outbox", OUTBOX_SQL),
    (
        "0001_init.sql",
        include_str!("../../migrations/0001_init.sql"),
    ),
];

const SERIALIZE_USER: &str = "SELECT pg_advisory_xact_lock(hashtext('notices'), $1::int)";
const SAME: &str = "SELECT EXISTS (SELECT 1 FROM notices WHERE to_user = $1 AND kind = $2 AND actor = $3 AND post IS NOT DISTINCT FROM $4 AND kind <> 'digest')";
const INSERT: &str = "INSERT INTO notices (to_user, kind, actor, post, count) VALUES ($1, $2, $3, $4, $5) RETURNING id";
const UNREAD: &str = "SELECT count(*) FROM notices WHERE to_user = $1 AND NOT read";
const LIST: &str = "SELECT id, to_user, kind, actor, post, read, count FROM notices WHERE to_user = $1 ORDER BY id DESC LIMIT $2";
const MARK: &str = "UPDATE notices SET read = true WHERE to_user = $1 AND NOT read";
const COUNTS: &str =
    "SELECT to_user, count(*) FROM notices WHERE NOT read GROUP BY to_user ORDER BY to_user";
const ENSURE: &str = "SELECT notices_ensure_partitions($1, $2)";
const DROP_OLDER: &str = "SELECT notices_drop_older($1)";

pub const KEEP_MONTHS: i32 = 3;

pub const AHEAD_MONTHS: i32 = 2;

pub struct PgNotices {
    pool: PgPool,
}

fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn small(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub fn kind_name(kind: NoticeKind) -> String {
    let name = match kind {
        NoticeKind::Followed => "followed",
        NoticeKind::Liked => "liked",
        NoticeKind::Commented => "commented",
        NoticeKind::NewPost => "new_post",
        NoticeKind::Digest => "digest",
    };
    name.to_string()
}

fn kind_of(name: &str) -> NoticeKind {
    match name {
        "followed" => NoticeKind::Followed,
        "liked" => NoticeKind::Liked,
        "commented" => NoticeKind::Commented,
        "new_post" => NoticeKind::NewPost,
        _ => NoticeKind::Digest,
    }
}

type Row = (i64, i64, String, i64, Option<i64>, bool, Option<i32>);

impl PgNotices {
    pub fn new(pool: PgPool) -> PgNotices {
        PgNotices { pool }
    }

    pub async fn ensure_partitions(&self, back: i32, ahead: i32) -> Result<i32, DbError> {
        let made: i32 = sqlx::query_scalar(ENSURE)
            .bind(back)
            .bind(ahead)
            .fetch_one(&self.pool)
            .await?;
        Ok(made)
    }

    pub async fn drop_older(&self, keep: i32) -> Result<i32, DbError> {
        let dropped: i32 = sqlx::query_scalar(DROP_OLDER)
            .bind(keep)
            .fetch_one(&self.pool)
            .await?;
        Ok(dropped)
    }

    async fn store(&self, draft: Draft) -> Result<Recorded, DbError> {
        let mut tx = self.pool.begin().await?;
        let to = big(draft.to.0);
        sqlx::query(SERIALIZE_USER)
            .bind(i32::try_from(draft.to.0 % (i32::MAX as u64)).unwrap_or(0))
            .execute(&mut *tx)
            .await?;
        let post = draft.post.map(|post| big(post.0));
        let same: bool = sqlx::query_scalar(SAME)
            .bind(to)
            .bind(kind_name(draft.kind))
            .bind(big(draft.actor.0))
            .bind(post)
            .fetch_one(&mut *tx)
            .await?;
        if same {
            return Ok(Recorded::Duplicate);
        }
        let id: i64 = sqlx::query_scalar(INSERT)
            .bind(to)
            .bind(kind_name(draft.kind))
            .bind(big(draft.actor.0))
            .bind(post)
            .bind(
                draft
                    .count
                    .map(|count| i32::try_from(count).unwrap_or(i32::MAX)),
            )
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Recorded::Stored(Notice {
            id: small(id),
            to: draft.to,
            kind: draft.kind,
            actor: draft.actor,
            post: draft.post,
            read: false,
            count: draft.count,
        }))
    }
}

fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl NoticeRepository for PgNotices {
    fn record(&self, draft: Draft) -> BoxFuture<'_, Recorded> {
        Box::pin(async move {
            match self.store(draft).await {
                Ok(recorded) => recorded,
                Err(error) => {
                    tracing::warn!("notify-service: уведомление не записано: {error}");
                    Recorded::Duplicate
                }
            }
        })
    }

    fn unread(&self, user: UserId) -> BoxFuture<'_, u32> {
        Box::pin(async move {
            let unread: i64 = sqlx::query_scalar(UNREAD)
                .bind(big(user.0))
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
            count(unread)
        })
    }

    fn list(&self, user: UserId, limit: usize) -> BoxFuture<'_, Vec<Notice>> {
        Box::pin(async move {
            let rows: Vec<Row> = sqlx::query_as(LIST)
                .bind(big(user.0))
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(id, to, kind, actor, post, read, count)| Notice {
                    id: small(id),
                    to: UserId(small(to)),
                    kind: kind_of(&kind),
                    actor: UserId(small(actor)),
                    post: post.map(|post| PostId(small(post))),
                    read,
                    count: count.and_then(|count| u32::try_from(count).ok()),
                })
                .collect()
        })
    }

    fn mark_read(&self, user: UserId) -> BoxFuture<'_, u32> {
        Box::pin(async move {
            let marked = sqlx::query(MARK)
                .bind(big(user.0))
                .execute(&self.pool)
                .await
                .map(|done| done.rows_affected())
                .unwrap_or(0);
            u32::try_from(marked).unwrap_or(u32::MAX)
        })
    }

    fn unread_counts(&self) -> BoxFuture<'_, Vec<(UserId, u32)>> {
        Box::pin(async move {
            let rows: Vec<(i64, i64)> = sqlx::query_as(COUNTS)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(user, unread)| (UserId(small(user)), count(unread)))
                .collect()
        })
    }
}
