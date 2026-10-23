// Очередь фоновых задач в таблице jobs (id, payload, status). `claim` забирает самую раннюю
// задачу со статусом 'queued', помечает её 'running' и возвращает её id; пусто, если задач нет.
// Несколько воркеров вызывают `claim` одновременно: ни одна задача не должна достаться двоим, и
// ни один воркер не должен уйти с пустыми руками, пока в очереди есть задачи.
// TODO: Напиши `claim`.

use sqlx::PgPool;

async fn claim(pool: &PgPool) -> Result<Option<i64>, sqlx::Error> {
    let _ = pool;
    Ok(None)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE jobs (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, payload text NOT NULL, status text NOT NULL DEFAULT 'queued');
        INSERT INTO jobs (payload) SELECT 'job' FROM generate_series(1, 3);
    ";

    #[tokio::test]
    async fn claims_in_order_then_nothing() {
        let db = TestDb::with_sql(SCHEMA).await;
        for want in 1..=3 {
            assert_eq!(claim(&db.pool).await.expect("claim"), Some(want), "задачи берутся по порядку id");
        }
        assert_eq!(claim(&db.pool).await.expect("claim"), None, "очередь пуста");
        assert_eq!(db.table("SELECT id FROM jobs WHERE status = 'running'").await.len(), 3, "взятые задачи помечены running");
    }

    #[tokio::test]
    async fn parallel_workers_get_different_jobs() {
        let db = TestDb::with_sql("
            CREATE TABLE jobs (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, payload text NOT NULL, status text NOT NULL DEFAULT 'queued');
            INSERT INTO jobs (payload) SELECT 'job' FROM generate_series(1, 8);
        ").await;
        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { claim(&pool).await }));
        }
        let mut got = Vec::new();
        for h in handles {
            match h.await.expect("задача упала").expect("claim") {
                Some(id) => got.push(id),
                None => panic!("воркер ушёл ни с чем, хотя задач восемь на восьмерых: занятая другим строка должна пропускаться, а не ожидаться"),
            }
        }
        got.sort();
        assert_eq!(got, [1, 2, 3, 4, 5, 6, 7, 8], "каждая задача досталась ровно одному воркеру");
    }
}
