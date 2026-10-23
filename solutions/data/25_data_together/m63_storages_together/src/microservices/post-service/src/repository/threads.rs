use contracts::{Cause, CommentId, Fact, PostCommented, PostId, UserId};
use service_runtime::StorageError;

use super::BoxFuture;
use super::pg::{PgPosts, big, small, storage};
use crate::common::CommentText;

const INSERT_REPLY: &str = "INSERT INTO comments (post, author, text, parent, at) SELECT $1, $2, $3, $4, $5 WHERE EXISTS (SELECT 1 FROM comments WHERE id = $4 AND post = $1 AND NOT removed) RETURNING id";
const THREAD: &str = "
WITH RECURSIVE tree (id, parent, author, text, removed, depth, path) AS (
    SELECT c.id, c.parent, c.author, c.text, c.removed, 1, ARRAY[c.id]
    FROM comments c WHERE c.post = $1 AND c.parent IS NULL
    UNION ALL
    SELECT c.id, c.parent, c.author, c.text, c.removed, t.depth + 1, t.path || c.id
    FROM comments c JOIN tree t ON c.parent = t.id
    WHERE t.depth < $2
) CYCLE id SET looped USING trail
SELECT t.id, coalesce(t.parent, 0) AS parent, t.author,
    CASE WHEN t.removed THEN '' ELSE t.text END AS text,
    t.depth, t.removed,
    (SELECT count(*) FROM comments r WHERE r.parent = t.id AND NOT r.removed) AS replies
FROM tree t
WHERE NOT t.looped
  AND (NOT t.removed OR EXISTS (SELECT 1 FROM comments r WHERE r.parent = t.id AND NOT r.removed))
ORDER BY t.path";
const LATEST: &str = "
SELECT p.id AS post, c.id, c.author, c.text
FROM unnest($1::bigint[]) AS p(id)
LEFT JOIN LATERAL (
    SELECT id, author, text FROM comments
    WHERE post = p.id AND NOT removed
    ORDER BY id DESC LIMIT 3
) c ON true
ORDER BY p.id, c.id DESC";
const TOP: &str = "
WITH scores AS (
    SELECT p.author, max(p.author_name) AS author_name,
        count(*) FILTER (WHERE l.at >= $1) AS fresh,
        count(*) FILTER (WHERE l.at < $1) AS past
    FROM likes l JOIN posts p ON p.id = l.post
    WHERE NOT p.deleted AND l.at >= $1 - $2
    GROUP BY p.author
), ranked AS (
    SELECT author, author_name, fresh, past,
        RANK() OVER (ORDER BY fresh DESC) AS place,
        RANK() OVER (ORDER BY past DESC) AS past_place
    FROM scores
)
SELECT author, author_name, place, fresh,
    CASE WHEN past = 0 THEN 0 ELSE past_place - place END AS shift
FROM ranked WHERE fresh > 0
ORDER BY place, author LIMIT $3";

pub const WEEK_MICROS: u64 = 7 * 24 * 60 * 60 * 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: CommentId,
    pub parent: Option<CommentId>,
    pub author: UserId,
    pub text: String,
    pub depth: u32,
    pub replies: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub id: CommentId,
    pub author: UserId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rank {
    pub author: UserId,
    pub author_name: String,
    pub place: u64,
    pub likes: u64,
    pub shift: i64,
}

pub trait ThreadQueries: Send + Sync {
    fn reply(
        &self,
        author: UserId,
        post: PostId,
        parent: CommentId,
        text: CommentText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<CommentId>, StorageError>>;
    fn thread(&self, post: PostId, depth: u32) -> BoxFuture<'_, Vec<Node>>;
    fn latest(&self, posts: Vec<PostId>) -> BoxFuture<'_, Vec<(PostId, Vec<Line>)>>;
    fn top(&self, now: u64, limit: u32) -> BoxFuture<'_, Vec<Rank>>;
}

type LatestRow = (i64, Option<i64>, Option<i64>, Option<String>);

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: i64,
    parent: i64,
    author: i64,
    text: String,
    depth: i32,
    removed: bool,
    replies: i64,
}

impl ThreadQueries for PgPosts {
    fn reply(
        &self,
        author: UserId,
        post: PostId,
        parent: CommentId,
        text: CommentText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<CommentId>, StorageError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let Some(found) = self.post(post).await.map_err(storage)? else {
                return Ok(None);
            };
            let mut tx = self.pool.begin().await.map_err(storage)?;
            let id: Option<i64> = sqlx::query_scalar(INSERT_REPLY)
                .bind(big(post.0))
                .bind(big(author.0))
                .bind(text.as_ref())
                .bind(big(parent.0))
                .bind(big(cause.at))
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
            let Some(id) = id else {
                return Ok(None);
            };
            let comment = CommentId(small(id));
            let fact = Fact::PostCommented(PostCommented {
                post_id: post,
                author_id: found.author,
                comment_id: comment,
                commenter_id: author,
            });
            self.commit(tx, Some(fact), &cause).await.map_err(storage)?;
            Ok(Some(comment))
        })
    }

    fn thread(&self, post: PostId, depth: u32) -> BoxFuture<'_, Vec<Node>> {
        Box::pin(async move {
            let rows: Vec<NodeRow> = sqlx::query_as(THREAD)
                .bind(big(post.0))
                .bind(i32::try_from(depth).unwrap_or(i32::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|row| Node {
                    id: CommentId(small(row.id)),
                    parent: (row.parent > 0).then(|| CommentId(small(row.parent))),
                    author: UserId(small(row.author)),
                    text: row.text,
                    depth: u32::try_from(row.depth).unwrap_or(0),
                    replies: small(row.replies),
                    deleted: row.removed,
                })
                .collect()
        })
    }

    fn latest(&self, posts: Vec<PostId>) -> BoxFuture<'_, Vec<(PostId, Vec<Line>)>> {
        Box::pin(async move {
            let ids: Vec<i64> = posts.iter().map(|post| big(post.0)).collect();
            let rows: Vec<LatestRow> = sqlx::query_as(LATEST)
                .bind(ids)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            let mut found: Vec<(PostId, Vec<Line>)> = Vec::new();
            for (post, id, author, text) in rows {
                let post = PostId(small(post));
                if found.last().is_none_or(|(last, _)| *last != post) {
                    found.push((post, Vec::new()));
                }
                if let (Some(id), Some(author), Some(text), Some((_, lines))) =
                    (id, author, text, found.last_mut())
                {
                    lines.push(Line {
                        id: CommentId(small(id)),
                        author: UserId(small(author)),
                        text,
                    });
                }
            }
            found
        })
    }

    fn top(&self, now: u64, limit: u32) -> BoxFuture<'_, Vec<Rank>> {
        Box::pin(async move {
            let rows: Vec<(i64, String, i64, i64, i64)> = sqlx::query_as(TOP)
                .bind(big(now.saturating_sub(WEEK_MICROS)))
                .bind(big(WEEK_MICROS))
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(author, author_name, place, likes, shift)| Rank {
                    author: UserId(small(author)),
                    author_name,
                    place: small(place),
                    likes: small(likes),
                    shift,
                })
                .collect()
        })
    }
}
