use sqlx::PgPool;

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Account {
    id: i64,
    username: String,
    email: String,
    bio: Option<String>,
}

#[derive(Debug)]
enum RegisterError {
    UsernameTaken,
    EmailTaken,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for RegisterError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            match db.constraint() {
                Some("users_username_uniq") => return RegisterError::UsernameTaken,
                Some("users_email_uniq") => return RegisterError::EmailTaken,
                _ => {}
            }
        }
        RegisterError::Db(e)
    }
}

const INSERT_USER: &str = "INSERT INTO users (username, email) VALUES ($1, $2) RETURNING id, username, email, bio";
const SELECT_USER: &str = "SELECT id, username, email, bio FROM users WHERE id = $1";
const UPDATE_BIO: &str = "UPDATE users SET bio = $2 WHERE id = $1";
const DELETE_USER: &str = "DELETE FROM users WHERE id = $1";

struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    fn new(pool: PgPool) -> Self {
        UserRepository { pool }
    }

    async fn create(&self, username: &str, email: &str) -> Result<Account, RegisterError> {
        let account = sqlx::query_as::<_, Account>(INSERT_USER)
            .bind(username)
            .bind(email)
            .fetch_one(&self.pool)
            .await?;
        Ok(account)
    }

    async fn find(&self, id: i64) -> Result<Option<Account>, sqlx::Error> {
        sqlx::query_as::<_, Account>(SELECT_USER).bind(id).fetch_optional(&self.pool).await
    }

    async fn set_bio(&self, id: i64, bio: Option<&str>) -> Result<bool, sqlx::Error> {
        let done = sqlx::query(UPDATE_BIO).bind(id).bind(bio).execute(&self.pool).await?;
        Ok(done.rows_affected() > 0)
    }

    async fn remove(&self, id: i64) -> Result<bool, sqlx::Error> {
        let done = sqlx::query(DELETE_USER).bind(id).execute(&self.pool).await?;
        Ok(done.rows_affected() > 0)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn create_and_find() {
        let db = TestDb::with_sql(SEED).await;
        let repo = UserRepository::new(db.pool.clone());
        let made = repo.create("grace", "grace@example.com").await.expect("создание должно пройти");
        assert_eq!(made, Account { id: 7, username: "grace".into(), email: "grace@example.com".into(), bio: None }, "созданный аккаунт целиком");
        assert_eq!(repo.find(7).await.unwrap(), Some(made), "найденный аккаунт совпадает с созданным");
        assert_eq!(repo.find(500).await.unwrap(), None, "несуществующий id: None");
    }

    #[tokio::test]
    async fn duplicates_are_domain_errors() {
        let db = TestDb::with_sql(SEED).await;
        let repo = UserRepository::new(db.pool.clone());
        assert!(matches!(repo.create("alice", "x@example.com").await, Err(RegisterError::UsernameTaken)), "ник занят");
        assert!(matches!(repo.create("xavier", "bob@example.com").await, Err(RegisterError::EmailTaken)), "почта занята");
    }

    #[tokio::test]
    async fn database_failure_keeps_its_cause() {
        let db = TestDb::with_sql(SEED).await;
        let repo = UserRepository::new(db.pool.clone());
        db.pool.close().await;
        match repo.create("zed", "zed@example.com").await {
            Err(RegisterError::Db(e)) => assert!(matches!(e, sqlx::Error::PoolClosed), "причина должна сохраниться внутри Db, а вышло {e:?}"),
            other => panic!("недоступная база это Db, а вышло {other:?}"),
        }
    }

    #[tokio::test]
    async fn bio_can_be_set_and_cleared() {
        let db = TestDb::with_sql(SEED).await;
        let repo = UserRepository::new(db.pool.clone());
        assert!(repo.set_bio(2, Some("Люблю кофе")).await.unwrap(), "bob существует");
        assert_eq!(repo.find(2).await.unwrap().unwrap().bio.as_deref(), Some("Люблю кофе"), "описание записалось");
        assert!(repo.set_bio(2, None).await.unwrap(), "стереть описание тоже обновление");
        assert_eq!(repo.find(2).await.unwrap().unwrap().bio, None, "после None описание пусто, а не пустая строка");
        assert!(!repo.set_bio(404, Some("x")).await.unwrap(), "нет такого пользователя: false");
    }

    #[tokio::test]
    async fn remove_takes_posts_with_it() {
        let db = TestDb::with_sql(SEED).await;
        let repo = UserRepository::new(db.pool.clone());
        assert!(repo.remove(1).await.unwrap(), "alice была");
        assert!(!repo.remove(1).await.unwrap(), "второй раз её уже нет");
        let n = db.table("SELECT count(*) FROM posts WHERE author_id = 1").await;
        assert_eq!(n.cell(0, 0), "0", "посты alice должны исчезнуть вместе с ней");
    }
}
