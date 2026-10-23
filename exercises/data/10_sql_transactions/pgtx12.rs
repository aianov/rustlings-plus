// Дружба между a и b увеличивает счётчик друзей у обоих. Функция `befriend` обновляет сначала
// a, потом b. Но когда alice и bob нажимают «добавить в друзья» друг другу одновременно, база
// объявляет взаимную блокировку (ошибка 40P01), и один запрос падает. (Пауза имитирует работу
// между обновлениями.) Нужно, чтобы шесть одновременных вызовов, в том числе встречных (1, 2)
// и (2, 1), все завершились успешно.
// TODO: Найди причину взаимной блокировки и исправь.

use sqlx::PgPool;

async fn befriend(pool: &PgPool, a: i64, b: i64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET friends = friends + 1 WHERE id = $1").bind(a).execute(&mut *tx).await?;
    sqlx::query("SELECT pg_sleep(0.1)").execute(&mut *tx).await?;
    sqlx::query("UPDATE users SET friends = friends + 1 WHERE id = $1").bind(b).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn crossing_requests_do_not_deadlock() {
        let db = TestDb::with_sql("CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, friends integer NOT NULL DEFAULT 0); INSERT INTO users DEFAULT VALUES; INSERT INTO users DEFAULT VALUES;").await;
        let mut handles = Vec::new();
        for i in 0..6 {
            let pool = db.pool.clone();
            let (a, b) = if i % 2 == 0 { (1, 2) } else { (2, 1) };
            handles.push(tokio::spawn(async move { befriend(&pool, a, b).await }));
        }
        for h in handles {
            if let Err(e) = h.await.expect("задача упала") {
                panic!("befriend вернул ошибку {e}: встречные запросы блокируют друг друга (40P01), блокировки нужно брать в одном порядке");
            }
        }
        let t = db.table("SELECT id, friends FROM users ORDER BY id").await;
        assert_eq!(t.rows, [["1", "6"], ["2", "6"]], "каждый из шести вызовов прибавил обоим по единице:\n{t}");
    }
}
