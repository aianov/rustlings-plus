// Редактор не должен зависать, если пост уже правит кто-то другой. Две функции, обе меняют
// body поста и возвращают Done, а если строка занята и ждать не стоит, Busy (не ошибку).
//  - `try_edit`: не ждёт совсем, занято значит Busy сразу.
//  - `edit_within`: ждёт блокировку не дольше wait_ms миллисекунд, потом Busy.
// Другие ошибки базы возвращаются как есть.
// TODO: Допиши обе функции.

use sqlx::PgPool;

#[derive(Debug, PartialEq)]
enum Outcome {
    Done,
    Busy,
}

async fn try_edit(pool: &PgPool, id: i64, body: &str) -> Result<Outcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM posts WHERE id = $1 FOR UPDATE").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET body = $2 WHERE id = $1").bind(id).bind(body).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Outcome::Done)
}

async fn edit_within(pool: &PgPool, id: i64, body: &str, wait_ms: i32) -> Result<Outcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM posts WHERE id = $1 FOR UPDATE").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET body = $2 WHERE id = $1").bind(id).bind(body).execute(&mut *tx).await?;
    tx.commit().await?;
    let _ = wait_ms;
    Ok(Outcome::Done)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('old');";

    async fn hold_row(db: &TestDb) -> sqlx::Transaction<'static, sqlx::Postgres> {
        let mut tx = db.pool.begin().await.expect("транзакция держателя");
        sqlx::query("SELECT id FROM posts WHERE id = 1 FOR UPDATE").execute(&mut *tx).await.expect("блокировка");
        tx
    }

    #[tokio::test]
    async fn try_edit_does_not_wait() {
        let db = TestDb::with_sql(SCHEMA).await;
        let holder = hold_row(&db).await;
        let res = tokio::time::timeout(std::time::Duration::from_secs(2), try_edit(&db.pool, 1, "mine"))
            .await
            .expect("try_edit ждёт блокировку, а должна сразу ответить Busy");
        assert_eq!(res.expect("запрос"), Outcome::Busy, "строка занята: ответ Busy");
        holder.rollback().await.unwrap();
        assert_eq!(try_edit(&db.pool, 1, "mine").await.expect("запрос"), Outcome::Done, "после освобождения правка проходит");
        assert_eq!(db.table("SELECT body FROM posts").await.cell(0, 0), "mine", "текст обновлён");
    }

    #[tokio::test]
    async fn edit_within_gives_up_after_the_limit() {
        let db = TestDb::with_sql(SCHEMA).await;
        let holder = hold_row(&db).await;
        let started = std::time::Instant::now();
        let res = tokio::time::timeout(std::time::Duration::from_secs(3), edit_within(&db.pool, 1, "late", 300))
            .await
            .expect("edit_within ждёт дольше лимита");
        assert_eq!(res.expect("запрос"), Outcome::Busy, "за 300 мс блокировка не освободилась: Busy");
        let waited = started.elapsed();
        assert!(waited >= std::time::Duration::from_millis(250), "ожидание {waited:?} слишком короткое: функция должна подождать лимит");
        holder.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn edit_within_waits_for_a_short_hold() {
        let db = TestDb::with_sql(SCHEMA).await;
        let holder = hold_row(&db).await;
        let release = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            holder.rollback().await.unwrap();
        });
        let res = edit_within(&db.pool, 1, "patient", 2000).await.expect("запрос");
        release.await.unwrap();
        assert_eq!(res, Outcome::Done, "держатель отпустил через 150 мс, лимит 2 секунды: правка должна пройти");
        assert_eq!(db.table("SELECT body FROM posts").await.cell(0, 0), "patient");
    }
}
