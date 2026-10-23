use std::time::Duration;

use db_engine::{
    DEFAULT_DB_URL, DbConfig, DbError, Migration, PG_URL_ENV, applied, checksum, connect,
    drop_schema, migrate,
};
use sqlx::{Connection, PgConnection};

fn config(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_mig_{name}_{}", std::process::id())).pool(4)
}

const FIRST: [Migration; 2] = [
    (
        "0001_pets.sql",
        "CREATE TABLE pets (id BIGINT PRIMARY KEY, name TEXT NOT NULL);",
    ),
    ("0002_seed.sql", "INSERT INTO pets VALUES (1, 'rex');"),
];

#[tokio::test]
async fn versions_checksums_and_one_runner_at_a_time() {
    let config = config("versions");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    let (one, two) = tokio::join!(migrate(&pool, &FIRST), migrate(&pool, &FIRST));
    let mut ran = [one.expect("первый"), two.expect("второй")];
    ran.sort();
    assert_eq!(
        ran,
        [
            vec![],
            vec![String::from("0001_pets.sql"), String::from("0002_seed.sql")]
        ],
        "advisory lock: мигрирует один, второй видит версии"
    );
    let rows = applied(&pool).await.expect("версии");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].checksum, checksum(FIRST[1].1));
    assert!(migrate(&pool, &FIRST).await.expect("повтор").is_empty());
    let changed = [
        FIRST[0],
        ("0002_seed.sql", "INSERT INTO pets VALUES (2, 'tom');"),
    ];
    assert!(
        matches!(migrate(&pool, &changed).await, Err(DbError::Migration(ref why)) if why.starts_with("0002_seed.sql")),
        "изменённый файл после применения это ошибка"
    );
    let unordered = [FIRST[1], FIRST[0]];
    assert!(matches!(
        migrate(&pool, &unordered).await,
        Err(DbError::Migration(..))
    ));
    let unnamed = [("pets.sql", "SELECT 1;")];
    assert!(matches!(
        migrate(&pool, &unnamed).await,
        Err(DbError::Migration(..))
    ));
    let broken = [FIRST[0], FIRST[1], ("0003_bad.sql", "CREATE TABLE (;")];
    assert!(
        matches!(migrate(&pool, &broken).await, Err(DbError::Query(ref why)) if why.starts_with("0003_bad.sql"))
    );
    assert_eq!(
        applied(&pool).await.expect("версии").len(),
        2,
        "упавшая миграция откатилась"
    );
    let concurrent = [
        FIRST[0],
        FIRST[1],
        (
            "0003_index.sql",
            "-- migrate: no-transaction\nCREATE INDEX CONCURRENTLY pets_by_name ON pets (name);\n",
        ),
    ];
    assert_eq!(
        migrate(&pool, &concurrent)
            .await
            .expect("CONCURRENTLY вне транзакции"),
        ["0003_index.sql"]
    );
    let _ = drop_schema(&config).await;
}

#[tokio::test]
async fn lock_timeout_is_retried() {
    let config = config("locks");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &FIRST).await.expect("схема");
    let mut holder = PgConnection::connect(config.url.expose())
        .await
        .expect("соединение");
    let mut tx = holder.begin().await.expect("транзакция");
    sqlx::query(&format!(
        "LOCK TABLE {}.pets IN ACCESS EXCLUSIVE MODE",
        config.schema
    ))
    .execute(&mut *tx)
    .await
    .expect("замок");
    let alter = [
        FIRST[0],
        FIRST[1],
        ("0003_age.sql", "ALTER TABLE pets ADD COLUMN age INTEGER;"),
    ];
    let release = async {
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        tx.rollback().await.expect("отпустить");
    };
    let (ran, ()) = tokio::join!(migrate(&pool, &alter), release);
    assert_eq!(
        ran.expect("после lock_timeout миграция повторилась"),
        ["0003_age.sql"]
    );
    let _ = drop_schema(&config).await;
}
