fn blockers_of() -> &'static str {
    "SELECT unnest(pg_blocking_pids($1))"
}

fn waiting_pids() -> &'static str {
    "SELECT DISTINCT pid FROM pg_locks WHERE NOT granted ORDER BY pid"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn finds_who_blocks_whom() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('x');").await;
        let mut holder = db.pool.begin().await.expect("держатель");
        let holder_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *holder).await.unwrap();
        sqlx::query("UPDATE posts SET body = 'held' WHERE id = 1").execute(&mut *holder).await.unwrap();
        let (tx_pid, rx_pid) = tokio::sync::oneshot::channel::<i32>();
        let pool = db.pool.clone();
        let waiter = tokio::spawn(async move {
            let mut conn = pool.acquire().await.expect("соединение ожидающего");
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *conn).await.unwrap();
            tx_pid.send(pid).unwrap();
            sqlx::query("UPDATE posts SET body = 'waiting' WHERE id = 1").execute(&mut *conn).await.unwrap();
        });
        let waiter_pid = rx_pid.await.expect("номер процесса ожидающего");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let blockers = sqlx::query_scalar::<_, i32>(blockers_of()).bind(waiter_pid).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("blockers_of: {e}"));
        assert_eq!(blockers, [holder_pid], "ожидающего блокирует процесс держателя");
        let waiting = sqlx::query_scalar::<_, i32>(waiting_pids()).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("waiting_pids: {e}"));
        assert!(waiting.contains(&waiter_pid), "процесс {waiter_pid} ждёт блокировку, а в списке ожидающих его нет: {waiting:?}");
        assert!(!waiting.contains(&holder_pid), "держатель не ждёт никого");
        holder.commit().await.unwrap();
        waiter.await.expect("ожидающий завершился");
    }
}
