// Блок «в тренде» считает лайки за сутки по каждому посту и каждый раз перечитывает всю таблицу
// лайков. Решили хранить результат в материализованном представлении `trending` (пост и число лайков за
// последние 24 часа, только посты с лайками) и обновлять раз в минуту. Первый же `REFRESH ... CONCURRENTLY`
// падает, а обычное обновление на время блокирует все чтения блока: пользователи видят белый экран.
// Обновление должно идти так, чтобы читатели не ждали.
// TODO: Почини `create_sql` и `refresh_sql`.

fn create_sql() -> &'static str {
    "CREATE MATERIALIZED VIEW trending AS
         SELECT post_id, count(*) AS likes_24h FROM likes
         WHERE created_at > now() - interval '24 hours'
         GROUP BY post_id"
}

fn refresh_sql() -> &'static str {
    "REFRESH MATERIALIZED VIEW trending"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "
        CREATE TABLE likes (post_id bigint NOT NULL, user_id bigint NOT NULL, created_at timestamptz NOT NULL);
        INSERT INTO likes VALUES
            (1, 1, now() - interval '1 hour'), (1, 2, now() - interval '2 hours'), (1, 3, now() - interval '3 hours'),
            (2, 1, now() - interval '5 hours'),
            (3, 1, now() - interval '3 days'), (3, 2, now() - interval '4 days');
    ";

    #[tokio::test]
    async fn view_holds_a_snapshot_until_refreshed() {
        let db = TestDb::with_sql(SEED).await;
        db.run(create_sql()).await;
        assert_eq!(db.table("SELECT post_id, likes_24h FROM trending ORDER BY post_id").await.rows, vec![vec!["1", "3"], vec!["2", "1"]], "в тренде посты 1 (3 лайка) и 2 (1 лайк); у поста 3 лайки старые");
        db.run("INSERT INTO likes VALUES (2, 2, now()), (2, 3, now()), (4, 1, now())").await;
        assert_eq!(db.table("SELECT count(*) FROM trending").await.cell(0, 0), "2", "до обновления представление показывает старый снимок");
        db.run(refresh_sql()).await;
        assert_eq!(db.table("SELECT post_id, likes_24h FROM trending ORDER BY post_id").await.rows, vec![vec!["1", "3"], vec!["2", "3"], vec!["4", "1"]], "после обновления видны новые лайки");
    }

    #[tokio::test]
    async fn refresh_does_not_block_readers() {
        let db = TestDb::with_sql(SEED).await;
        db.run(create_sql()).await;
        let mut reader = db.pool.begin().await.expect("транзакция читателя");
        sqlx::query("SELECT * FROM trending").fetch_all(&mut *reader).await.expect("читатель прочитал представление и не закрывает транзакцию");
        let mut writer = db.pool.acquire().await.expect("соединение для обновления");
        sqlx::raw_sql("SET lock_timeout = '1500ms'").execute(&mut *writer).await.expect("lock_timeout");
        let res = sqlx::raw_sql(refresh_sql()).execute(&mut *writer).await;
        reader.rollback().await.expect("откат читателя");
        let err = res.err().map(|e| e.to_string());
        assert!(err.is_none(), "обновление не должно ждать читателей, а оно упало: {err:?} (обычный REFRESH требует эксклюзивной блокировки, CONCURRENTLY нет)");
    }
}
