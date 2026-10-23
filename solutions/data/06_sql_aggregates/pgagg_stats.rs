fn profile_counters() -> &'static str {
    "SELECT (SELECT count(*) FROM follows WHERE followee_id = $1) AS followers,
            (SELECT count(*) FROM follows WHERE follower_id = $1) AS following,
            (SELECT count(*) FROM posts WHERE author_id = $1) AS posts,
            (SELECT count(*) FROM likes l JOIN posts p ON p.id = l.post_id WHERE p.author_id = $1) AS likes_received"
}

fn busy_authors() -> &'static str {
    "SELECT u.username, count(DISTINCT p.id) AS posts, count(l.user_id) AS likes
     FROM users u
     JOIN posts p ON p.author_id = u.id
     LEFT JOIN likes l ON l.post_id = p.id
     GROUP BY u.id
     HAVING count(DISTINCT p.id) >= 2 AND count(l.user_id) >= 3
     ORDER BY u.username"
}

fn daily_posts() -> &'static str {
    "SELECT d::date AS day, count(p.id) AS posts
     FROM generate_series(date '2026-09-28', date '2026-10-03', interval '1 day') AS d
     LEFT JOIN posts p ON p.created_at::date = d::date
     GROUP BY d
     ORDER BY d"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn counters(db: &TestDb, user: i64) -> (i64, i64, i64, i64) {
        sqlx::query_as::<_, (i64, i64, i64, i64)>(profile_counters())
            .bind(user)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("profile_counters({user}): {e}"))
    }

    #[tokio::test]
    async fn counters_do_not_multiply() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(counters(&db, 1).await, (3, 2, 3, 8), "alice: 3 подписчика, 2 подписки, 3 поста, 8 лайков; цифры не должны перемножаться");
        assert_eq!(counters(&db, 2).await, (2, 1, 2, 3), "bob: 2 подписчика, 1 подписка, 2 поста, 3 лайка");
    }

    #[tokio::test]
    async fn counters_of_a_quiet_user() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(counters(&db, 5).await, (0, 1, 0, 0), "у eve нет ни постов, ни подписчиков: нули, а не пропавшая строка или NULL");
    }

    #[tokio::test]
    async fn busy_authors_need_both_conditions() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(busy_authors()).await;
        assert_eq!(t.columns, ["username", "posts", "likes"], "нужны столбцы username, posts, likes");
        assert_eq!(
            t.rows,
            [["alice", "3", "8"], ["bob", "2", "3"]],
            "carol пишет часто, но лайков не получала; dave лайков ждёт и постов мало:\n{t}"
        );
    }

    #[tokio::test]
    async fn every_day_present() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(daily_posts()).await;
        assert_eq!(t.columns, ["day", "posts"], "нужны столбцы day и posts");
        assert_eq!(
            t.rows,
            [["2026-09-28", "2"], ["2026-09-29", "1"], ["2026-09-30", "0"], ["2026-10-01", "2"], ["2026-10-02", "0"], ["2026-10-03", "3"]],
            "шесть дней, в пустые стоит 0 (а не 1 и не пропуск):\n{t}"
        );
    }
}
