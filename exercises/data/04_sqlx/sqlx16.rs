// Весь SQL приложения в одном месте и за одним интерфейсом называется репозиторием. Остальной
// код не знает, что за ним стоит Postgres: он вызывает `repo.find(5)`. Запросы лучше держать в
// константах наверху файла, а не размазывать по методам.
// Реализуй UserRepository:
//  - `create`: добавить пользователя и вернуть Account целиком (ошибки как в RegisterError);
//  - `find`: аккаунт по id или None;
//  - `set_bio`: поставить или стереть (None) описание; true, если пользователь есть;
//  - `remove`: удалить пользователя вместе с его постами (это делает схема); true, если он был.
// TODO: Допиши структуру и методы.

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

struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    fn new(pool: PgPool) -> Self {
        todo!()
    }

    async fn create(&self, username: &str, email: &str) -> Result<Account, RegisterError> {
        todo!()
    }

    async fn find(&self, id: i64) -> Result<Option<Account>, sqlx::Error> {
        todo!()
    }

    async fn set_bio(&self, id: i64, bio: Option<&str>) -> Result<bool, sqlx::Error> {
        todo!()
    }

    async fn remove(&self, id: i64) -> Result<bool, sqlx::Error> {
        todo!()
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
