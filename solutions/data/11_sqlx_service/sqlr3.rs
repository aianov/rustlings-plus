use sqlx::{Acquire, PgConnection, Postgres};

async fn like_post(conn: &mut PgConnection, post_id: i64, user_id: i64) -> Result<bool, sqlx::Error> {
    let inserted = sqlx::query("INSERT INTO likes (post_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(post_id)
        .bind(user_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if inserted == 1 {
        sqlx::query("UPDATE posts SET likes_count = likes_count + 1 WHERE id = $1")
            .bind(post_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(inserted == 1)
}

async fn like_many<'a, A>(db: A, post_id: i64, users: &[i64]) -> Result<u32, sqlx::Error>
where
    A: Acquire<'a, Database = Postgres>, {
    let mut tx = db.begin().await?;
    let mut added = 0;
    for user in users {
        if like_post(&mut tx, post_id, *user).await? {
            added += 1;
        }
    }
    tx.commit().await?;
    Ok(added)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn counters(db: &TestDb, post: i64) -> (String, String) {
        let c = db.table(&format!("SELECT likes_count::text, (SELECT count(*) FROM likes WHERE post_id = {post})::text FROM posts WHERE id = {post}")).await;
        (c.cell(0, 0).to_string(), c.cell(0, 1).to_string())
    }

    #[tokio::test]
    async fn works_on_pool_and_connection() {
        let db = TestDb::with_sql(SEED).await;
        let added = like_many(&db.pool, 2, &[2, 3, 4]).await.expect("лайки на пуле");
        assert_eq!(added, 2, "лайк пользователя 2 уже был, новых два");
        assert_eq!(counters(&db, 2).await, ("3".to_string(), "3".to_string()), "счётчик и число лайков должны совпасть");
        let mut conn = db.pool.acquire().await.unwrap();
        let added = like_many(&mut *conn, 3, &[1]).await.expect("лайки на соединении");
        assert_eq!(added, 1, "один новый лайк");
    }

    #[tokio::test]
    async fn failure_in_the_middle_rolls_everything_back() {
        let db = TestDb::with_sql(SEED).await;
        let res = like_many(&db.pool, 3, &[1, 999, 2]).await;
        assert!(res.is_err(), "пользователя 999 нет, внешний ключ должен отвергнуть лайк");
        assert_eq!(
            counters(&db, 3).await,
            ("0".to_string(), "0".to_string()),
            "лайк пользователя 1 остался, хотя вся партия закончилась ошибкой: на пуле нужна своя транзакция"
        );
    }

    #[tokio::test]
    async fn inside_outer_transaction_only_own_part_is_rolled_back() {
        let db = TestDb::with_sql(SEED).await;
        let mut tx = db.pool.begin().await.unwrap();
        like_many(&mut tx, 3, &[1]).await.expect("первая партия внутри транзакции");
        let res = like_many(&mut tx, 3, &[2, 999]).await;
        assert!(res.is_err(), "вторая партия должна упасть на пользователе 999");
        tx.commit().await.expect("внешняя транзакция должна остаться рабочей: ошибка внутренней части откатывается до точки сохранения");
        assert_eq!(
            counters(&db, 3).await,
            ("1".to_string(), "1".to_string()),
            "после коммита должен остаться лайк первой партии и не должно быть лайка пользователя 2 из упавшей"
        );
    }
}
