use contracts::PostId;
use journal::JournalError;

use super::BoxFuture;
use super::pg::{PgPosts, PostRow, small, storage};
use crate::common::Post;

const FIND: &str = "
SELECT p.id, p.author, p.author_name, p.body AS text, p.likes, p.at, p.deleted,
    ts_rank(p.search, q) AS rank,
    ts_headline('russian', p.body, q, 'StartSel=«, StopSel=», MaxFragments=2, MinWords=3, MaxWords=12') AS headline
FROM posts p, websearch_to_tsquery('russian', $1) q
WHERE p.search @@ q AND NOT p.deleted
ORDER BY rank DESC, p.id DESC
LIMIT $2";
const TRENDS: &str = "SELECT post, likes FROM post_trends ORDER BY likes DESC, post DESC LIMIT $1";
const REFRESH: &str = "SELECT refresh_post_trends()";

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub post: Post,
    pub rank: f32,
    pub headline: String,
}

#[derive(sqlx::FromRow)]
struct HitRow {
    #[sqlx(flatten)]
    post: PostRow,
    rank: f32,
    headline: String,
}

pub trait FullText: Send + Sync {
    fn find(&self, query: String, limit: u32) -> BoxFuture<'_, Result<Vec<Hit>, JournalError>>;
    fn trends(&self, limit: u32) -> BoxFuture<'_, Vec<(PostId, usize)>>;
    fn refresh_trends(&self) -> BoxFuture<'_, Result<(), JournalError>>;
}

impl FullText for PgPosts {
    fn find(&self, query: String, limit: u32) -> BoxFuture<'_, Result<Vec<Hit>, JournalError>> {
        Box::pin(async move {
            let rows: Vec<HitRow> = sqlx::query_as(FIND)
                .bind(query)
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
                .map_err(storage)?;
            let marks: Vec<(f32, String)> = rows
                .iter()
                .map(|row| (row.rank, row.headline.clone()))
                .collect();
            let posts = rows.into_iter().map(|row| row.post).collect();
            let posts = self.assemble(posts).await.map_err(storage)?;
            Ok(posts
                .into_iter()
                .zip(marks)
                .map(|(post, (rank, headline))| Hit {
                    post,
                    rank,
                    headline,
                })
                .collect())
        })
    }

    fn trends(&self, limit: u32) -> BoxFuture<'_, Vec<(PostId, usize)>> {
        Box::pin(async move {
            let rows: Vec<(i64, i64)> = sqlx::query_as(TRENDS)
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(post, likes)| (PostId(small(post)), usize::try_from(likes).unwrap_or(0)))
                .collect()
        })
    }

    fn refresh_trends(&self) -> BoxFuture<'_, Result<(), JournalError>> {
        Box::pin(async move {
            sqlx::query(REFRESH)
                .execute(&self.pool)
                .await
                .map_err(storage)?;
            Ok(())
        })
    }
}
