// Очередь фоновых задач в таблице jobs (id, payload, status, visible_at, attempts). Воркеров много,
// и любой может упасть, не закончив задачу.
//  - `enqueue(payload, delay_ms)`: добавляет задачу, которая станет доступна через delay_ms
//    миллисекунд (visible_at); возвращает id.
//  - `claim(lease_ms)`: берёт самую раннюю доступную задачу (status 'queued' и visible_at уже наступил),
//    «арендует» её: сдвигает visible_at на lease_ms вперёд и увеличивает attempts. Возвращает
//    (id, payload, attempts) или None. Параллельные воркеры не получают одну задачу.
//  - `ack(id)`: воркер закончил; ставит status 'done'. true, если задача была в очереди.
// Если воркер не вызвал ack, после конца аренды задача снова станет доступна (повторная попытка).
// TODO: Напиши три функции.

use sqlx::PgPool;

async fn enqueue(pool: &PgPool, payload: &str, delay_ms: i32) -> Result<i64, sqlx::Error> {
    todo!()
}

async fn claim(pool: &PgPool, lease_ms: i32) -> Result<Option<(i64, String, i32)>, sqlx::Error> {
    todo!()
}

async fn ack(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE jobs (
            id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            payload text NOT NULL,
            status text NOT NULL DEFAULT 'queued',
            visible_at timestamptz NOT NULL DEFAULT now(),
            attempts integer NOT NULL DEFAULT 0
        );
    ";

    #[tokio::test]
    async fn order_and_delay() {
        let db = TestDb::with_sql(SCHEMA).await;
        let a = enqueue(&db.pool, "a", 0).await.expect("enqueue");
        let b = enqueue(&db.pool, "b", 0).await.expect("enqueue");
        let c = enqueue(&db.pool, "c", 400).await.expect("enqueue");
        assert_eq!(claim(&db.pool, 5000).await.expect("claim").map(|j| j.0), Some(a), "первой берётся самая ранняя задача");
        assert_eq!(claim(&db.pool, 5000).await.expect("claim").map(|j| j.0), Some(b), "второй вторая");
        assert_eq!(claim(&db.pool, 5000).await.expect("claim"), None, "задача c ещё не доступна (отложена на 400 мс)");
        tokio::time::sleep(std::time::Duration::from_millis(450)).await;
        let job = claim(&db.pool, 5000).await.expect("claim").expect("к этому времени c должна стать доступной");
        assert_eq!((job.0, job.1.as_str(), job.2), (c, "c", 1), "отложенная задача, первая попытка");
    }

    #[tokio::test]
    async fn parallel_workers_never_share_a_job() {
        let db = TestDb::with_sql(SCHEMA).await;
        for i in 0..10 {
            enqueue(&db.pool, &format!("job{i}"), 0).await.expect("enqueue");
        }
        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { claim(&pool, 5000).await }));
        }
        let mut ids = Vec::new();
        for h in handles {
            ids.push(h.await.expect("задача упала").expect("claim").expect("задач десять на восьмерых, пустых рук быть не должно: занятые строки нужно пропускать").0);
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 8, "восемь воркеров должны получить восемь разных задач");
    }

    #[tokio::test]
    async fn lost_job_comes_back_after_the_lease() {
        let db = TestDb::with_sql(SCHEMA).await;
        let id = enqueue(&db.pool, "x", 0).await.expect("enqueue");
        let first = claim(&db.pool, 300).await.expect("claim").expect("задача есть");
        assert_eq!((first.0, first.2), (id, 1), "первая попытка");
        assert_eq!(claim(&db.pool, 300).await.expect("claim"), None, "пока аренда не кончилась, задачу никто не получит");
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let again = claim(&db.pool, 300).await.expect("claim").expect("воркер не вызвал ack, после конца аренды задача должна вернуться");
        assert_eq!((again.0, again.2), (id, 2), "та же задача, вторая попытка");
    }

    #[tokio::test]
    async fn acked_job_never_returns() {
        let db = TestDb::with_sql(SCHEMA).await;
        let id = enqueue(&db.pool, "y", 0).await.expect("enqueue");
        claim(&db.pool, 200).await.expect("claim").expect("задача есть");
        assert!(ack(&db.pool, id).await.expect("ack"), "первый ack подтверждает задачу");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(claim(&db.pool, 200).await.expect("claim"), None, "выполненная задача не возвращается после конца аренды");
        assert!(!ack(&db.pool, id).await.expect("ack"), "повторный ack ничего не находит");
    }
}
