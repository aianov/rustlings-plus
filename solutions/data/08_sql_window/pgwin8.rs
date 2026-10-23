fn post_gaps() -> &'static str {
    "SELECT id, author_id,
            round(extract(epoch FROM created_at - lag(created_at) OVER w) / 3600) AS hours_since_prev,
            lead(id) OVER w AS next_post_id
     FROM posts
     WINDOW w AS (PARTITION BY author_id ORDER BY created_at)
     ORDER BY author_id, created_at"
}

fn daily_growth() -> &'static str {
    "SELECT day, posts, posts - lag(posts) OVER (ORDER BY day) AS delta
     FROM (SELECT created_at::date AS day, count(*) AS posts FROM posts GROUP BY 1) d
     ORDER BY day"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn gaps_between_posts_of_one_author() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_gaps()).await;
        assert_eq!(t.columns, ["id", "author_id", "hours_since_prev", "next_post_id"], "нужны столбцы id, author_id, hours_since_prev, next_post_id");
        assert_eq!(
            t.rows,
            [
                ["1", "1", "NULL", "8"], ["8", "1", "84", "3"], ["3", "1", "14", "NULL"],
                ["10", "2", "NULL", "5"], ["5", "2", "26", "2"], ["2", "2", "23", "NULL"],
                ["7", "3", "NULL", "4"], ["4", "3", "98", "NULL"],
                ["6", "4", "NULL", "NULL"], ["9", "5", "NULL", "NULL"],
            ],
            "предыдущий и следующий считаются внутри автора: у первого поста нет предыдущего, у последнего следующего (NULL):\n{t}"
        );
    }

    #[tokio::test]
    async fn day_over_day_change() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(daily_growth()).await;
        assert_eq!(t.columns, ["day", "posts", "delta"], "нужны столбцы day, posts, delta");
        assert_eq!(
            t.rows,
            [
                ["2026-10-01", "3", "NULL"], ["2026-10-02", "1", "-2"], ["2026-10-03", "1", "0"], ["2026-10-04", "1", "0"],
                ["2026-10-05", "2", "1"], ["2026-10-06", "1", "-1"], ["2026-10-07", "1", "0"],
            ],
            "в первый день разницы нет (NULL), дальше текущий день минус предыдущий:\n{t}"
        );
    }

    #[tokio::test]
    async fn gaps_follow_new_posts() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, likes, created_at) VALUES (4, 'again', 0, '2026-10-07 19:30:00+00')").await;
        let t = db.table(post_gaps()).await;
        let new_row = t.rows.iter().find(|r| r[0] == "11").expect("новый пост в выдаче");
        assert_eq!((new_row[2].as_str(), new_row[3].as_str()), ("12", "NULL"), "новый пост dave: через 12 часов после hello, следующего нет:\n{t}");
        let old_row = t.rows.iter().find(|r| r[0] == "6").expect("пост 6 в выдаче");
        assert_eq!(old_row[3], "11", "у поста 6 теперь есть следующий:\n{t}");
    }
}
