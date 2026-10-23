fn everything_written() -> &'static str {
    "SELECT 'post' AS kind, body FROM posts WHERE author_id = $1
     UNION ALL
     SELECT 'comment', body FROM comments WHERE author_id = $1
     ORDER BY kind, body"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn written(db: &TestDb, user: i64) -> Vec<(String, String)> {
        sqlx::query_as::<_, (String, String)>(everything_written())
            .bind(user)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("everything_written({user}): {e}"))
    }

    fn pair(kind: &str, body: &str) -> (String, String) {
        (kind.to_string(), body.to_string())
    }

    #[tokio::test]
    async fn posts_and_comments_together() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            written(&db, 2).await,
            [
                pair("comment", "first"),
                pair("comment", "nice"),
                pair("comment", "nice"),
                pair("comment", "same here"),
                pair("comment", "yes"),
                pair("post", "coffee"),
                pair("post", "monday"),
                pair("post", "tuesday"),
            ],
            "у bob три поста и пять комментариев; 'nice' он написал дважды, и обе строки нужны (UNION склеил бы их в одну)"
        );
    }

    #[tokio::test]
    async fn user_with_posts_only() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(written(&db, 5).await, [pair("comment", "great post"), pair("post", "quiet")], "у eve один комментарий и один пост");
        assert!(written(&db, 7).await.is_empty(), "grace ничего не писала");
    }
}
