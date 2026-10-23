use sqlx::PgPool;

#[derive(Debug)]
enum TagError {
    AlreadyExists,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for TagError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => TagError::AlreadyExists,
            _ => TagError::Db(e),
        }
    }
}

async fn create_tag(pool: &PgPool, name: &str) -> Result<i64, TagError> {
    let id = sqlx::query_scalar("INSERT INTO tags (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn new_tag_gets_an_id() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(create_tag(&db.pool, "async").await.expect("новый тег должен создаться"), 4, "в базе три тега, новый получает id 4");
    }

    #[tokio::test]
    async fn duplicate_is_already_exists() {
        let db = TestDb::with_sql(SEED).await;
        let got = create_tag(&db.pool, "rust").await;
        assert!(matches!(got, Err(TagError::AlreadyExists)), "тег rust уже есть, ждали AlreadyExists, а вышло {got:?}");
    }

    #[tokio::test]
    async fn other_errors_are_not_disguised() {
        let db = TestDb::with_sql(SEED).await;
        let got = create_tag(&db.pool, "").await;
        assert!(matches!(got, Err(TagError::Db(_))), "пустое имя нарушает CHECK, это не «уже существует»: {got:?}");
        db.pool.close().await;
        match create_tag(&db.pool, "later").await {
            Err(TagError::Db(e)) => assert!(matches!(e, sqlx::Error::PoolClosed), "закрытый пул должен дать PoolClosed внутри Db, а вышло {e:?}"),
            other => panic!("недоступная база это Db, а вышло {other:?}"),
        }
    }

    #[tokio::test]
    async fn race_for_one_name_has_one_winner() {
        let db = TestDb::with_sql(SEED).await;
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            tasks.push(tokio::spawn(async move { create_tag(&pool, "contest").await }));
        }
        let (mut won, mut lost) = (0, 0);
        for t in tasks {
            match t.await.unwrap() {
                Ok(_) => won += 1,
                Err(TagError::AlreadyExists) => lost += 1,
                Err(e) => panic!("в гонке допустимы только победа и AlreadyExists, а вышло {e:?}"),
            }
        }
        assert_eq!((won, lost), (1, 11), "из 12 одновременных попыток должна пройти ровно одна");
    }
}
