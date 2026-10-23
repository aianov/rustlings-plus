use db_engine::{
    DEFAULT_DB_URL, DbConfig, DbError, PG_URL_ENV, Violation, apply, connect, drop_schema,
    good_schema,
};

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS owners (id BIGINT PRIMARY KEY, name TEXT NOT NULL CONSTRAINT owners_name_key UNIQUE);
CREATE TABLE IF NOT EXISTS pets (id BIGINT PRIMARY KEY, owner BIGINT NOT NULL CONSTRAINT pets_owner_fkey REFERENCES owners (id), age INT CONSTRAINT pets_age_check CHECK (age >= 0));
";

fn config() -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_engine_{}", std::process::id())).pool(2)
}

#[test]
fn schema_names_are_identifiers() {
    assert!(good_schema("users") && good_schema("t_users_42") && good_schema("_x"));
    for bad in ["", "Users", "1users", "users;drop", "a-b", &"a".repeat(64)] {
        assert!(!good_schema(bad), "{bad}");
    }
}

#[tokio::test]
async fn constraints_become_domain_errors() {
    let config = config();
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    let migrations = [("0001_pets.sql", SCHEMA_SQL)];
    assert_eq!(apply(&pool, &migrations).await.expect("схема"), 1);
    assert_eq!(
        apply(&pool, &migrations).await.expect("повтор"),
        1,
        "файл идемпотентен"
    );
    let path: String = sqlx::query_scalar("SELECT current_schema()")
        .fetch_one(&pool)
        .await
        .expect("схема");
    assert_eq!(path, config.schema, "у сервиса своя схема");
    sqlx::query("INSERT INTO owners (id, name) VALUES (1, 'alice')")
        .execute(&pool)
        .await
        .expect("строка");
    let twin = sqlx::query("INSERT INTO owners (id, name) VALUES (2, 'alice')")
        .execute(&pool)
        .await;
    let twin = DbError::from(twin.expect_err("ник занят"));
    assert!(twin.is_unique("owners_name_key"), "{twin:?}");
    let orphan = sqlx::query("INSERT INTO pets (id, owner) VALUES (1, 99)")
        .execute(&pool)
        .await;
    assert!(matches!(
        DbError::from(orphan.expect_err("хозяина нет")),
        DbError::Constraint { violation: Violation::ForeignKey, ref constraint } if constraint == "pets_owner_fkey"
    ));
    let negative = sqlx::query("INSERT INTO pets (id, owner, age) VALUES (2, 1, -1)")
        .execute(&pool)
        .await;
    assert!(matches!(
        DbError::from(negative.expect_err("возраст")),
        DbError::Constraint {
            violation: Violation::Check,
            ..
        }
    ));
    let broken = apply(&pool, &[("0002_bad.sql", "CREATE TABLE (")]).await;
    assert!(
        matches!(broken, Err(DbError::Query(ref why)) if why.starts_with("0002_bad.sql")),
        "{broken:?}"
    );
    pool.close().await;
    drop_schema(&config).await.expect("схема убрана");
    assert!(matches!(
        connect(&DbConfig::new(DEFAULT_DB_URL, "Плохая")).await,
        Err(DbError::BadSchema(..))
    ));
}
