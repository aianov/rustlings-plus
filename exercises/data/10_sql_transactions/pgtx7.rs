// У пользователей есть баллы, которые можно тратить: `spend` списывает amount, если хватает,
// и возвращает true; иначе false и баллы не меняются. Проверка есть, но под нагрузкой баланс уходит
// в минус: при 100 баллах и двенадцати одновременных списаниях по 30 успешными должны быть
// ровно три, а остаток 10. (Пауза в коде имитирует работу между проверкой и списанием.)
// TODO: Найди, как две проверки обе прошли, и исправь.

use sqlx::PgPool;

async fn spend(pool: &PgPool, user_id: i64, amount: i32) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let points: i32 = sqlx::query_scalar("SELECT points FROM users WHERE id = $1").bind(user_id).fetch_one(&mut *tx).await?;
    if points < amount {
        return Ok(false);
    }
    sqlx::query("SELECT pg_sleep(0.05)").execute(&mut *tx).await?;
    sqlx::query("UPDATE users SET points = points - $2 WHERE id = $1").bind(user_id).bind(amount).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn balance_never_goes_negative() {
        let db = TestDb::with_sql("CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, points integer NOT NULL); INSERT INTO users (points) VALUES (100);").await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { spend(&pool, 1, 30).await }));
        }
        let mut ok = 0;
        for h in handles {
            if h.await.expect("задача упала").expect("spend вернул ошибку") {
                ok += 1;
            }
        }
        let left = db.table("SELECT points FROM users WHERE id = 1").await.cell(0, 0).to_string();
        assert_eq!(ok, 3, "при 100 баллах успешных списаний по 30 ровно три, а их {ok}");
        assert_eq!(left, "10", "остаток должен быть 10, а не {left}: проверка и списание не должны разделяться чужими запросами");
    }
}
