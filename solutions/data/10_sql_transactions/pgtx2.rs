use sqlx::PgPool;

async fn register_all(pool: &PgPool, names: &[&str]) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut added = 0;
    for name in names {
        let res = sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        added += res.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(added)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, username text NOT NULL UNIQUE);
        INSERT INTO users (username) VALUES ('bob');
    ";

    #[tokio::test]
    async fn busy_names_are_skipped() {
        let db = TestDb::with_sql(SCHEMA).await;
        let added = register_all(&db.pool, &["alice", "bob", "carol", "alice"]).await.expect("импорт не должен падать из-за занятых ников");
        assert_eq!(added, 2, "новых ников два: alice и carol (bob занят, второй alice повтор)");
        let t = db.table("SELECT username FROM users ORDER BY username").await;
        assert_eq!(t.rows, [["alice"], ["bob"], ["carol"]], "в базе должны остаться все три: после первой ошибки транзакция становится «aborted», и COMMIT откатывает всё:\n{t}");
    }
}
