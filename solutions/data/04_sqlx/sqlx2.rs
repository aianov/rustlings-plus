use sqlx::{PgPool, Row};

async fn usernames_with_status(pool: &PgPool, status: &str, min_len: i32) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query("SELECT username FROM users WHERE status = $1 AND length(username) >= $2 ORDER BY username")
        .bind(status)
        .bind(min_len)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("username")).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn filters_by_status_and_length() {
        let db = TestDb::with_sql(SEED).await;
        let got = usernames_with_status(&db.pool, "active", 4).await.expect("запрос должен выполниться");
        assert_eq!(got, ["alice"], "активные с ником от 4 символов: только alice (bob и eve короче)");
        let got = usernames_with_status(&db.pool, "pending", 1).await.expect("запрос должен выполниться");
        assert_eq!(got, ["dave", "frank"], "ожидающие по алфавиту");
    }

    #[tokio::test]
    async fn user_input_is_data_not_sql() {
        let db = TestDb::with_sql(SEED).await;
        let got = usernames_with_status(&db.pool, "x' OR 'a'='a", 0).await.expect("странный статус это просто строка, запрос не должен падать");
        assert!(got.is_empty(), "статус с кавычками должен искаться как обычная строка, а не менять запрос: {got:?}");
        let got = usernames_with_status(&db.pool, "it's", 0).await.expect("одинокая кавычка не должна ломать запрос");
        assert!(got.is_empty(), "такого статуса нет: {got:?}");
    }

    #[tokio::test]
    async fn nothing_long_enough() {
        let db = TestDb::with_sql(SEED).await;
        let got = usernames_with_status(&db.pool, "active", 50).await.expect("запрос должен выполниться");
        assert!(got.is_empty(), "ников длиннее 50 символов нет");
    }
}
