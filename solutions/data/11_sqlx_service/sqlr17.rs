use sqlx::PgPool;

fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

async fn search_text(pool: &PgPool, q: &str) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM posts WHERE body ILIKE $1 ORDER BY id")
        .bind(format!("%{}%", escape_like(q)))
        .fetch_all(pool)
        .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn found(db: &TestDb, q: &str) -> Vec<i64> {
        search_text(&db.pool, q).await.unwrap_or_else(|e| panic!("поиск `{q}` закончился ошибкой базы: {e}"))
    }

    #[tokio::test]
    async fn percent_is_just_a_percent() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(found(&db, "100%").await, vec![2], "«100%» есть только в посте 2, пост «1000 рублей» подходить не должен");
        assert_eq!(found(&db, "1000").await, vec![3], "обычный текст ищется как раньше");
    }

    #[tokio::test]
    async fn underscore_is_just_an_underscore() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body) VALUES (1, 'axb тест')").await;
        assert_eq!(found(&db, "a_b").await, vec![4], "«a_b» должно искаться буквально: пост «axb тест» подходить не должен");
    }

    #[tokio::test]
    async fn backslash_is_just_a_backslash() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(found(&db, r"C:\dir").await, vec![5], "обратная косая черта в запросе должна искаться как символ");
        assert_eq!(found(&db, r"dir\").await, vec![5], "запрос, оканчивающийся косой чертой, не должен падать с ошибкой шаблона");
    }

    #[tokio::test]
    async fn case_is_ignored_and_empty_matches_all() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(found(&db, "RUST").await, vec![1, 2, 7], "регистр не учитывается");
        assert_eq!(found(&db, "").await, vec![1, 2, 3, 4, 5, 6, 7, 8], "пустая строка подходит всем постам");
    }
}
