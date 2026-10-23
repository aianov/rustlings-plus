use sqlx::PgPool;
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
struct FeedItem {
    post_id: i64,
    body: String,
    author: String,
}

async fn feed(pool: &PgPool, limit: i64) -> Result<Vec<FeedItem>, sqlx::Error> {
    let posts: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT id, author_id, body FROM posts WHERE status = 'published' ORDER BY created_at DESC, id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut ids: Vec<i64> = posts.iter().map(|p| p.1).collect();
    ids.sort_unstable();
    ids.dedup();
    let names: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>("SELECT id, username FROM users WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
    Ok(posts
        .into_iter()
        .map(|(post_id, author_id, body)| FeedItem { post_id, body, author: names.get(&author_id).cloned().unwrap_or_default() })
        .collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn feed_is_correct() {
        let db = TestDb::with_sql(SEED).await;
        let items = feed(&db.pool, 5).await.expect("лента");
        let ids: Vec<i64> = items.iter().map(|i| i.post_id).collect();
        assert_eq!(ids, vec![6, 2, 5, 3, 8], "лента должна идти от новых к старым, без черновиков");
        let authors: Vec<&str> = items.iter().map(|i| i.author.as_str()).collect();
        assert_eq!(authors, vec!["dmitry", "anna", "clara", "boris", "anna"], "у каждого поста должен быть его автор");
        assert_eq!(items[0], FeedItem { post_id: 6, body: "Просто хороший день".to_string(), author: "dmitry".to_string() }, "первый элемент ленты");
    }

    #[tokio::test]
    async fn number_of_queries_does_not_grow_with_the_feed() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, status, created_at) SELECT 1 + g % 4, 'p' || g, 'published', now() - (g || ' minutes')::interval FROM generate_series(1, 60) g").await;
        let (pool, counter) = db.counted_pool(2).await;
        let small = feed(&pool, 5).await.expect("короткая лента");
        let small_queries = counter.get();
        counter.reset();
        let big = feed(&pool, 60).await.expect("длинная лента");
        let big_queries = counter.get();
        assert_eq!((small.len(), big.len()), (5, 60), "ленты должны быть запрошенной длины");
        assert!(small_queries <= 2, "лента из 5 постов выполнила {small_queries} запросов, должно быть не больше двух");
        assert!(big_queries <= 2, "лента из 60 постов выполнила {big_queries} запросов: на каждый пост ходить за автором в базу нельзя (N+1)");
    }
}
