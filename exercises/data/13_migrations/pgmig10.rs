// После шага expand новый столбец `body` пуст у двухсот тысяч старых строк, и их надо заполнить
// значением из `text`. Один огромный `UPDATE` держит блокировки на всех строках, пока не закончит, раздувает
// таблицу двойным размером и давит на репликацию. Заполнять надо пачками: небольшая выборка, отдельная
// транзакция на каждую пачку, короткая пауза между ними не нужна, но каждая пачка должна завершаться сама.
// Функция возвращает, сколько строк заполнено, и повторный запуск возвращает 0.
// TODO: Допиши `backfill`: `batch` строк за раз, пока есть что заполнять.

use sqlx::PgPool;

async fn backfill(pool: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    let _ = batch;
    let done = sqlx::query("UPDATE posts SET body = text WHERE body IS NULL").execute(pool).await?;
    Ok(done.rows_affected())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SETUP: &str = "
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, text text NOT NULL, body text);
        INSERT INTO posts (text) SELECT 'пост ' || g FROM generate_series(1, 50000) g;
    ";

    #[tokio::test]
    async fn every_row_is_filled() {
        let db = TestDb::with_sql(SETUP).await;
        let n = backfill(&db.pool, 5000).await.expect("заполнение");
        assert_eq!(n, 50_000, "должно быть заполнено 50000 строк");
        let t = db.table("SELECT count(*) FROM posts WHERE body IS DISTINCT FROM text").await;
        assert_eq!(t.cell(0, 0), "0", "у каждой строки body должен совпадать с text");
        assert_eq!(backfill(&db.pool, 5000).await.expect("повторное заполнение"), 0, "повторно заполнять нечего");
    }

    #[tokio::test]
    async fn each_batch_is_its_own_transaction() {
        let db = TestDb::with_sql(SETUP).await;
        backfill(&db.pool, 5000).await.expect("заполнение пачками по 5000");
        let t = db.table("SELECT count(DISTINCT xmin::text) FROM posts").await;
        let transactions: i64 = t.cell(0, 0).parse().unwrap();
        assert!((10..=12).contains(&transactions), "50000 строк пачками по 5000 это десять транзакций, а изменения оставили следы {transactions} разных транзакций: один огромный UPDATE или все пачки в одной транзакции");
    }

    #[tokio::test]
    async fn batch_size_is_respected() {
        let db = TestDb::with_sql(SETUP).await;
        backfill(&db.pool, 20_000).await.expect("заполнение пачками по 20000");
        let t = db.table("SELECT count(DISTINCT xmin::text) FROM posts").await;
        let transactions: i64 = t.cell(0, 0).parse().unwrap();
        assert!((3..=4).contains(&transactions), "50000 строк пачками по 20000 это три транзакции, а следы {transactions}");
    }
}
