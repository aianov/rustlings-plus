// В пятницу вечером миграция `ALTER TABLE posts ADD COLUMN lang text` встала: на таблице висела чужая
// транзакция (аналитик открыл `BEGIN`, прочитал таблицу и ушёл на обед). ALTER ждёт, пока его
// пустят, а за ним встали ВСЕ новые чтения таблицы: ALTER стоит в очереди первым, и никто не вправе
// обогнать его. Через минуту лежит весь сервис, хотя сам ALTER ещё ничего не сделал.
// TODO: Научи `run_ddl` не ждать замок дольше трёхсот миллисекунд, а при неудаче повторять попытку
// до `attempts` раз с короткой паузой. Если замок так и не получен, ошибка `LockBusy`. Любая другая
// ошибка возвращается сразу.

use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug)]
enum DdlError {
    LockBusy,
    Db(sqlx::Error),
}

async fn run_ddl(pool: &PgPool, ddl: &str, attempts: u32) -> Result<(), DdlError> {
    let _ = attempts;
    sqlx::raw_sql(ddl).execute(pool).await.map_err(DdlError::Db)?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SETUP: &str = "CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('a'), ('b');";

    async fn wait_until_blocked(pool: &PgPool, marker: &str) {
        for _ in 0..40 {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND query LIKE $1 AND query NOT LIKE '%pg_stat_activity%'")
                .bind(format!("%{marker}%"))
                .fetch_one(pool)
                .await
                .unwrap();
            if n > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn has_column(db: &TestDb, name: &str) -> bool {
        db.table(&format!("SELECT count(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = '{name}'")).await.cell(0, 0) == "1"
    }

    #[tokio::test]
    async fn readers_are_not_stuck_behind_the_alter() {
        let db = TestDb::with_sql(SETUP).await;
        let mut holder = db.pool.begin().await.unwrap();
        sqlx::query("SELECT count(*) FROM posts").fetch_one(&mut *holder).await.unwrap();
        let ddl = run_ddl(&db.pool, "ALTER TABLE posts ADD COLUMN lang_a text /* pgmig5_a */", 3);
        let scenario = async {
            wait_until_blocked(&db.pool, "pgmig5_a").await;
            let reader = tokio::time::timeout(Duration::from_secs(3), sqlx::query_scalar::<_, i64>("SELECT count(*) FROM posts").fetch_one(&db.pool)).await;
            holder.rollback().await.unwrap();
            reader.is_ok()
        };
        let (_, reader_ok) = tokio::join!(ddl, scenario);
        assert!(reader_ok, "обычное чтение таблицы не получило ответа за 3 секунды: оно стоит в очереди за ALTER, который сам ждёт чужую транзакцию. ALTER не должен ждать замок долго");
    }

    #[tokio::test]
    async fn retries_until_the_lock_frees_up() {
        let db = TestDb::with_sql(SETUP).await;
        let mut holder = db.pool.begin().await.unwrap();
        sqlx::query("SELECT count(*) FROM posts").fetch_one(&mut *holder).await.unwrap();
        let ddl = tokio::time::timeout(Duration::from_secs(8), run_ddl(&db.pool, "ALTER TABLE posts ADD COLUMN lang_b text /* pgmig5_b */", 30));
        let release = async {
            tokio::time::sleep(Duration::from_millis(800)).await;
            holder.rollback().await.unwrap();
        };
        let (res, _) = tokio::join!(ddl, release);
        match res.expect("run_ddl не завершилась после освобождения таблицы") {
            Ok(()) => {}
            Err(DdlError::LockBusy) => panic!("попыток хватало с запасом, а замок так и не получен"),
            Err(DdlError::Db(e)) => panic!("неожиданная ошибка базы: {e}"),
        }
        assert!(has_column(&db, "lang_b").await, "после успешной попытки столбец должен появиться");
    }

    #[tokio::test]
    async fn gives_up_with_lock_busy() {
        let db = TestDb::with_sql(SETUP).await;
        let mut holder = db.pool.begin().await.unwrap();
        sqlx::query("SELECT count(*) FROM posts").fetch_one(&mut *holder).await.unwrap();
        let res = tokio::time::timeout(Duration::from_secs(4), run_ddl(&db.pool, "ALTER TABLE posts ADD COLUMN lang_c text /* pgmig5_c */", 2)).await;
        holder.rollback().await.unwrap();
        let res = res.expect("две попытки по триста миллисекунд должны закончиться раньше четырёх секунд: run_ddl ждёт замок бесконечно");
        assert!(matches!(res, Err(DdlError::LockBusy)), "когда замок так и не получен, ожидалась ошибка LockBusy");
        assert!(!has_column(&db, "lang_c").await, "столбец не должен появиться");
    }

    #[tokio::test]
    async fn other_errors_are_not_retried() {
        let db = TestDb::with_sql(SETUP).await;
        let started = std::time::Instant::now();
        let res = run_ddl(&db.pool, "ALTER TABLE posts ADD COLUMN", 20).await;
        assert!(matches!(res, Err(DdlError::Db(_))), "синтаксическая ошибка это ошибка базы, а не занятый замок");
        assert!(started.elapsed() < Duration::from_secs(2), "ошибку, которая не лечится повтором, надо возвращать сразу");
    }
}
