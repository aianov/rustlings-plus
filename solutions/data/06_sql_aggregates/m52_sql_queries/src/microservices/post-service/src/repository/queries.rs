use std::collections::HashSet;

use contracts::{PostId, UserId};

use super::pg::{PgPosts, PostRow, big, small};
use super::{BoxFuture, MemoryPosts, PostQueries};
use crate::common::Post;

const BY_IDS: &str = "SELECT p.id, p.author, p.author_name, p.text, p.likes, p.at, p.deleted, (l.user_id IS NOT NULL) AS liked FROM posts p LEFT JOIN likes l ON l.post = p.id AND l.user_id = $2 WHERE p.id = ANY($1) AND NOT p.deleted AND NOT EXISTS (SELECT 1 FROM unnest($3::bigint[]) AS hidden(author) WHERE hidden.author = p.author)";
const AUTHOR_TOTALS: &str = "SELECT count(*) AS posts, coalesce(sum(likes), 0)::bigint AS likes FROM posts WHERE author = $1 AND NOT deleted";
const LIKED: &str = "SELECT post FROM likes WHERE user_id = $1 AND post = ANY($2)";
const DAILY: &str = "
WITH bounds AS (
    SELECT date_trunc('day', to_timestamp($1::double precision / 1000000) AT TIME ZONE 'UTC') AS today
)
SELECT to_char(d.day, 'YYYY-MM-DD') AS day,
    (SELECT count(*) FROM posts p
        WHERE NOT p.deleted
          AND p.at >= extract(epoch FROM d.day)::bigint * 1000000
          AND p.at < extract(epoch FROM d.day + interval '1 day')::bigint * 1000000) AS posts,
    (SELECT count(*) FROM likes l
        WHERE l.at >= extract(epoch FROM d.day)::bigint * 1000000
          AND l.at < extract(epoch FROM d.day + interval '1 day')::bigint * 1000000) AS likes
FROM bounds, generate_series(bounds.today - make_interval(days => $2 - 1), bounds.today, interval '1 day') AS d(day)
ORDER BY d.day";

pub const DAY_MICROS: u64 = 24 * 60 * 60 * 1_000_000;

#[derive(sqlx::FromRow)]
struct Hydrated {
    #[sqlx(flatten)]
    post: PostRow,
    liked: bool,
}

fn order(ids: &[PostId], found: Vec<(Post, bool)>) -> Vec<(Post, bool)> {
    let mut found = found;
    found.sort_by_key(|(post, _)| {
        ids.iter()
            .position(|id| *id == post.id)
            .unwrap_or(usize::MAX)
    });
    found
}

impl PostQueries for PgPosts {
    fn by_ids(
        &self,
        viewer: Option<UserId>,
        ids: Vec<PostId>,
        hidden: Vec<UserId>,
    ) -> BoxFuture<'_, Vec<(Post, bool)>> {
        Box::pin(async move {
            let wanted: Vec<i64> = ids.iter().map(|id| big(id.0)).collect();
            let hidden: Vec<i64> = hidden.iter().map(|user| big(user.0)).collect();
            let rows: Vec<Hydrated> = sqlx::query_as(BY_IDS)
                .bind(wanted)
                .bind(viewer.map(|user| big(user.0)))
                .bind(hidden)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            let mut found = Vec::with_capacity(rows.len());
            for row in rows {
                if let Ok(post) = self.full(row.post).await {
                    found.push((post, row.liked));
                }
            }
            order(&ids, found)
        })
    }

    fn author_totals(&self, author: UserId) -> BoxFuture<'_, (u64, u64)> {
        Box::pin(async move {
            let row: (i64, i64) = sqlx::query_as(AUTHOR_TOTALS)
                .bind(big(author.0))
                .fetch_one(&self.pool)
                .await
                .unwrap_or((0, 0));
            (small(row.0), small(row.1))
        })
    }

    fn liked(&self, viewer: UserId, posts: Vec<PostId>) -> BoxFuture<'_, Vec<PostId>> {
        Box::pin(async move {
            let ids: Vec<i64> = posts.iter().map(|id| big(id.0)).collect();
            let rows: Vec<i64> = sqlx::query_scalar(LIKED)
                .bind(big(viewer.0))
                .bind(ids)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter().map(|id| PostId(small(id))).collect()
        })
    }

    fn daily(&self, days: u32, now: u64) -> BoxFuture<'_, Vec<(String, u64, u64)>> {
        Box::pin(async move {
            let rows: Vec<(String, i64, i64)> = sqlx::query_as(DAILY)
                .bind(big(now))
                .bind(i32::try_from(days.max(1)).unwrap_or(i32::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(day, posts, likes)| (day, small(posts), small(likes)))
                .collect()
        })
    }
}

pub fn day_name(day: u64) -> String {
    let days = i64::try_from(day).unwrap_or(0);
    let (year, month, date) = civil(days);
    format!("{year:04}-{month:02}-{date:02}")
}

fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let date = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, date)
}

impl PostQueries for MemoryPosts {
    fn by_ids(
        &self,
        viewer: Option<UserId>,
        ids: Vec<PostId>,
        hidden: Vec<UserId>,
    ) -> BoxFuture<'_, Vec<(Post, bool)>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let hidden: HashSet<UserId> = hidden.into_iter().collect();
            ids.iter()
                .filter_map(|id| state.post(*id).cloned())
                .filter(|post| !hidden.contains(&post.author))
                .map(|post| {
                    let liked = viewer.is_some_and(|viewer| {
                        state
                            .likers
                            .get(&post.id)
                            .is_some_and(|likers| likers.contains(&viewer))
                    });
                    (post, liked)
                })
                .collect()
        })
    }

    fn author_totals(&self, author: UserId) -> BoxFuture<'_, (u64, u64)> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let own = state
                .posts
                .iter()
                .filter(|post| post.author == author && !post.deleted);
            own.fold((0, 0), |(posts, likes), post| {
                (posts + 1, likes + post.likes as u64)
            })
        })
    }

    fn liked(&self, viewer: UserId, posts: Vec<PostId>) -> BoxFuture<'_, Vec<PostId>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            posts
                .into_iter()
                .filter(|post| {
                    state
                        .likers
                        .get(post)
                        .is_some_and(|likers| likers.contains(&viewer))
                })
                .collect()
        })
    }

    fn daily(&self, days: u32, now: u64) -> BoxFuture<'_, Vec<(String, u64, u64)>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let today = now / DAY_MICROS;
            let first = today.saturating_sub(u64::from(days.max(1)) - 1);
            (first..=today)
                .map(|day| {
                    let inside = |at: u64| at / DAY_MICROS == day;
                    let posts = state
                        .posts
                        .iter()
                        .filter(|post| !post.deleted && inside(post.at))
                        .count() as u64;
                    let likes = state.likes.iter().filter(|(_, at)| inside(*at)).count() as u64;
                    (day_name(day), posts, likes)
                })
                .collect()
        })
    }
}
