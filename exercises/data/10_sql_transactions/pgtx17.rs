// Воркер разбирает очередь: забирает задачу, «обрабатывает» (пауза в коде), отмечает готовой,
// и так, пока очередь не опустеет. Возвращает id всех задач, которые обработал он сам. Четыре
// воркера запущены одновременно, но некоторые задачи обрабатываются дважды. Каждая должна
// обрабатываться ровно один раз.
// TODO: Найди, как две воркеры берут одну задачу, и исправь `claim`.

use sqlx::PgPool;

async fn claim(pool: &PgPool) -> Result<Option<i64>, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM jobs WHERE status = 'queued' ORDER BY id LIMIT 1").fetch_optional(pool).await?;
    if let Some(id) = id {
        sqlx::query("SELECT pg_sleep(0.02)").execute(pool).await?;
        sqlx::query("UPDATE jobs SET status = 'running' WHERE id = $1").bind(id).execute(pool).await?;
    }
    Ok(id)
}

async fn work_all(pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    let mut handled = Vec::new();
    while let Some(id) = claim(pool).await? {
        sqlx::query("SELECT pg_sleep(0.02)").execute(pool).await?;
        sqlx::query("UPDATE jobs SET status = 'done' WHERE id = $1").bind(id).execute(pool).await?;
        handled.push(id);
    }
    Ok(handled)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn every_job_is_handled_exactly_once() {
        let db = TestDb::with_sql("
            CREATE TABLE jobs (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, payload text NOT NULL, status text NOT NULL DEFAULT 'queued');
            INSERT INTO jobs (payload) SELECT 'job' FROM generate_series(1, 24);
        ").await;
        let mut handles = Vec::new();
        for _ in 0..4 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { work_all(&pool).await }));
        }
        let mut all = Vec::new();
        for h in handles {
            all.extend(h.await.expect("задача упала").expect("work_all"));
        }
        let total = all.len();
        all.sort();
        let mut unique = all.clone();
        unique.dedup();
        assert_eq!(total, unique.len(), "задачи обработаны повторно: {} обработок на {} разных задач", total, unique.len());
        assert_eq!(unique, (1..=24).collect::<Vec<i64>>(), "все двадцать четыре задачи должны быть обработаны");
        assert_eq!(db.table("SELECT id FROM jobs WHERE status = 'done'").await.len(), 24, "все задачи в статусе done");
    }
}
