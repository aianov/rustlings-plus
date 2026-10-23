use sqlx::PgPool;

async fn peek_uncommitted(pool: &PgPool) -> Result<(i64, i64, bool), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO posts (body) VALUES ('peek')").execute(&mut *tx).await?;
    let inside: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(&mut *tx).await?;
    let outside: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(pool).await?;
    let mine: bool = sqlx::query_scalar("SELECT xmin::text::bigint = txid_current() % 4294967296 FROM posts WHERE body = 'peek'")
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
    Ok((inside, outside, mine))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn each_connection_sees_its_own_versions() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('a'), ('b');").await;
        let (inside, outside, mine) = peek_uncommitted(&db.pool).await.expect("запросы должны выполниться");
        assert_eq!(inside, 3, "своя транзакция видит свою вставку: две старые строки и новая");
        assert_eq!(outside, 2, "другое соединение незакоммиченную строку не видит");
        assert!(mine, "xmin новой строки это номер той транзакции, что её создала");
        assert_eq!(db.table("SELECT * FROM posts").await.len(), 2, "после отката в таблице снова две строки");
    }
}
