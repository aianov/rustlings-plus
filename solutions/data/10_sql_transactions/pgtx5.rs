fn report_begin() -> &'static str {
    "BEGIN ISOLATION LEVEL REPEATABLE READ"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn report_sees_one_snapshot() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY); INSERT INTO posts DEFAULT VALUES; INSERT INTO posts DEFAULT VALUES;").await;
        let mut report = db.pool.acquire().await.expect("соединение");
        sqlx::query(report_begin()).execute(&mut *report).await.expect("начало транзакции");
        let first: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(&mut *report).await.unwrap();
        sqlx::query("INSERT INTO posts DEFAULT VALUES").execute(&db.pool).await.expect("параллельная публикация");
        let second: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(&mut *report).await.unwrap();
        sqlx::query("COMMIT").execute(&mut *report).await.unwrap();
        assert_eq!(first, 2, "первый запрос видит две строки");
        assert_eq!(second, first, "второй запрос того же отчёта увидел чужой коммит: уровень READ COMMITTED берёт новый снимок на каждую команду");
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(&mut *report).await.unwrap();
        assert_eq!(after, 3, "после окончания транзакции новая строка видна");
    }
}
