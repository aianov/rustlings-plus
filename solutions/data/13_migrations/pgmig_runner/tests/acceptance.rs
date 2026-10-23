use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use migrator::{MigrateError, Migration, Migrator, Options, State, parse_dir};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL").unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Env {
    pool: PgPool,
    schema: String,
    key: i64,
}

impl Env {
    async fn new() -> Env {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let schema = format!("mig_{}_{}_{}", std::process::id(), n, nanos);
        let mut conn = PgConnection::connect(&pg_url())
            .await
            .unwrap_or_else(|e| panic!("Postgres курса не отвечает ({}): {e}. Запусти docker compose up -d", pg_url()));
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\"")).execute(&mut conn).await.unwrap();
        let options = PgConnectOptions::from_str(&pg_url()).unwrap().options([("search_path", schema.as_str()), ("timezone", "UTC")]);
        let pool = PgPoolOptions::new().max_connections(8).acquire_timeout(Duration::from_secs(8)).connect_with(options).await.unwrap();
        let key = 8_000_000 + (std::process::id() as i64 % 1000) * 1000 + n as i64;
        Env { pool, schema, key }
    }

    fn options(&self) -> Options {
        Options { lock_key: self.key, lock_timeout_ms: 200, attempts: 30, retry_pause_ms: 50 }
    }

    fn migrator(&self) -> Migrator {
        Migrator::new(self.pool.clone(), self.options())
    }

    async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap()
    }

    async fn table_exists(&self, name: &str) -> bool {
        self.scalar(&format!("SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = '{name}'")).await == 1
    }

    async fn locks_held(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND granted AND ((classid::bigint << 32) | objid::bigint) = $1")
            .bind(self.key)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let url = pg_url();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                if let Ok(mut conn) = PgConnection::connect(&url).await {
                    let _ = sqlx::raw_sql(&format!("DROP SCHEMA \"{schema}\" CASCADE")).execute(&mut conn).await;
                }
            });
        })
        .join();
    }
}

fn mig(version: i64, name: &str, sql: &str) -> Migration {
    Migration { version, name: name.to_string(), sql: sql.to_string() }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        let dir = std::env::temp_dir().join(format!("migrator_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn file(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parse_dir_orders_by_version_and_ignores_other_files() {
    let dir = TempDir::new();
    dir.file("0002_add_posts.sql", "CREATE TABLE posts (id int);");
    dir.file("0001_init.sql", "CREATE TABLE users (id int);");
    dir.file("10_late.sql", "SELECT 1;");
    dir.file("README.md", "не миграция");
    dir.file("notes.txt", "тоже не миграция");
    let found = parse_dir(&dir.0).unwrap_or_else(|e| panic!("разбор каталога: {e:?}"));
    let got: Vec<(i64, &str)> = found.iter().map(|m| (m.version, m.name.as_str())).collect();
    assert_eq!(got, vec![(1, "init"), (2, "add_posts"), (10, "late")], "версии по возрастанию как числа (10 после 2), имя без номера и расширения, прочие файлы пропускаются");
    assert_eq!(found[0].sql, "CREATE TABLE users (id int);", "текст файла должен быть прочитан как есть");
}

#[test]
fn parse_dir_rejects_bad_names_and_duplicates() {
    let dir = TempDir::new();
    dir.file("init.sql", "SELECT 1;");
    match parse_dir(&dir.0) {
        Err(MigrateError::BadFileName(name)) => assert_eq!(name, "init.sql", "в ошибке должно быть имя файла"),
        other => panic!("файл без номера должен дать BadFileName, а вышло {other:?}"),
    }
    let dir = TempDir::new();
    dir.file("0001_a.sql", "SELECT 1;");
    dir.file("1_b.sql", "SELECT 2;");
    assert!(matches!(parse_dir(&dir.0), Err(MigrateError::DuplicateVersion(1))), "0001 и 1 это одна версия, ожидалось DuplicateVersion(1)");
    let dir = TempDir::new();
    dir.file("-1_x.sql", "SELECT 1;");
    assert!(matches!(parse_dir(&dir.0), Err(MigrateError::BadFileName(_))), "номер должен состоять только из цифр");
}

#[tokio::test]
async fn applies_in_order_and_remembers() {
    let env = Env::new().await;
    let m = env.migrator();
    let list = vec![
        mig(3, "seed", "INSERT INTO books (id, title) VALUES (1, 'Dune')"),
        mig(1, "books", "CREATE TABLE books (id bigint PRIMARY KEY)"),
        mig(2, "title", "ALTER TABLE books ADD COLUMN title text"),
    ];
    let report = m.run(&list).await.unwrap_or_else(|e| panic!("запуск: {e:?}"));
    assert_eq!(report.applied, vec![1, 2, 3], "миграции применяются по возрастанию версии, а не в порядке списка");
    assert_eq!(env.scalar("SELECT count(*) FROM schema_migrations").await, 3, "в журнале три записи");
    let again = m.run(&list).await.unwrap_or_else(|e| panic!("повторный запуск: {e:?}"));
    assert!(again.applied.is_empty(), "повторный запуск ничего не применяет, а применил {:?}", again.applied);
    assert_eq!(env.locks_held().await, 0, "замок раннера должен быть снят");
}

#[tokio::test]
async fn status_reports_pending_and_applied() {
    let env = Env::new().await;
    let m = env.migrator();
    let list = vec![mig(1, "books", "CREATE TABLE books (id int)"), mig(2, "more", "CREATE TABLE more (id int)")];
    let before = m.status(&list).await.unwrap_or_else(|e| panic!("статус до запуска: {e:?}"));
    assert!(before.iter().all(|s| s.state == State::Pending), "до первого запуска таблицы журнала нет, всё ожидает: {before:?}");
    assert!(!env.table_exists("schema_migrations").await, "status не должен создавать таблицу журнала");
    m.run(&list[..1]).await.expect("первая миграция");
    let after = m.status(&list).await.expect("статус после");
    assert_eq!((after[0].state.clone(), after[1].state.clone()), (State::Applied, State::Pending), "первая применена, вторая ждёт");
    assert_eq!((after[0].version, after[1].name.as_str()), (1, "more"), "статус содержит версию и имя");
}

#[tokio::test]
async fn failure_is_atomic_and_stops_the_run() {
    let env = Env::new().await;
    let m = env.migrator();
    let broken = vec![
        mig(1, "a", "CREATE TABLE a (id int)"),
        mig(2, "broken", "CREATE TABLE b (id int); SELECT 1/0"),
        mig(3, "c", "CREATE TABLE c (id int)"),
    ];
    match m.run(&broken).await {
        Err(MigrateError::Failed { version, error }) => {
            assert_eq!(version, 2, "упасть должна миграция 2");
            assert!(error.to_string().contains("division by zero"), "в ошибке должна быть причина: {error}");
        }
        other => panic!("ожидалось Failed для версии 2, а вышло {other:?}"),
    }
    assert!(env.table_exists("a").await, "миграция 1 применена и остаётся");
    assert!(!env.table_exists("b").await, "половина упавшей миграции не должна остаться");
    assert!(!env.table_exists("c").await, "после ошибки следующие миграции не запускаются");
    assert_eq!(env.scalar("SELECT count(*) FROM schema_migrations").await, 1, "записана только миграция 1");
    assert_eq!(env.locks_held().await, 0, "замок раннера должен быть снят и после ошибки");
    let fixed = vec![broken[0].clone(), mig(2, "broken", "CREATE TABLE b (id int)"), broken[2].clone()];
    let report = m.run(&fixed).await.unwrap_or_else(|e| panic!("упавшая миграция не записывалась, исправленный файл должен применяться заново: {e:?}"));
    assert_eq!(report.applied, vec![2, 3], "после исправления применяются версии 2 и 3");
}

#[tokio::test]
async fn edited_files_are_caught() {
    let env = Env::new().await;
    let m = env.migrator();
    m.run(&[mig(1, "a", "CREATE TABLE a (id int)")]).await.expect("первый запуск");
    let edited = vec![mig(1, "a", "CREATE TABLE a (id bigint)"), mig(2, "b", "CREATE TABLE b (id int)")];
    match m.run(&edited).await {
        Err(MigrateError::Drift { version }) => assert_eq!(version, 1, "правили версию 1"),
        other => panic!("правка применённого файла должна давать Drift, а вышло {other:?}"),
    }
    assert!(!env.table_exists("b").await, "пока есть расхождение, новые миграции не применяются");
}

#[tokio::test]
async fn missing_and_out_of_order_files() {
    let env = Env::new().await;
    let m = env.migrator();
    let one = mig(1, "a", "CREATE TABLE a (id int)");
    let three = mig(3, "c", "CREATE TABLE c (id int)");
    m.run(&[one.clone(), three.clone()]).await.expect("версии 1 и 3");
    let two = mig(2, "b", "CREATE TABLE b (id int)");
    match m.run(&[one.clone(), two, three]).await {
        Err(MigrateError::OutOfOrder { version }) => assert_eq!(version, 2, "версия 2 новее применённой 1, но старше уже применённой 3"),
        other => panic!("ожидалось OutOfOrder для версии 2, а вышло {other:?}"),
    }
    assert!(!env.table_exists("b").await, "миграция вне очереди не применяется");
    match m.run(&[one]).await {
        Err(MigrateError::MissingFile { version }) => assert_eq!(version, 3, "применённая версия 3 пропала из каталога"),
        other => panic!("ожидалось MissingFile для версии 3, а вышло {other:?}"),
    }
}

#[tokio::test]
async fn several_copies_migrate_only_once() {
    let env = Env::new().await;
    let list = vec![mig(1, "slow", "CREATE TABLE a (id int); SELECT pg_sleep(0.3)"), mig(2, "b", "CREATE TABLE b (id int)")];
    let copies = [env.migrator(), env.migrator(), env.migrator(), env.migrator(), env.migrator()];
    let results = tokio::join!(
        copies[0].run(&list),
        copies[1].run(&list),
        copies[2].run(&list),
        copies[3].run(&list),
        copies[4].run(&list),
    );
    let mut total = 0;
    for result in [results.0, results.1, results.2, results.3, results.4] {
        match result {
            Ok(report) => total += report.applied.len(),
            Err(e) => panic!("одна из копий упала: {e:?}. Остальные должны ждать замок и затем увидеть, что делать нечего"),
        }
    }
    assert_eq!(total, 2, "каждая миграция применена ровно один раз на все копии");
    assert_eq!(env.locks_held().await, 0, "замок должен быть снят");
}

#[tokio::test]
async fn files_without_transaction_may_build_indexes_concurrently() {
    let env = Env::new().await;
    let m = env.migrator();
    let list = vec![
        mig(1, "base", "CREATE TABLE posts (id bigint PRIMARY KEY, author_id bigint, created_at timestamptz)"),
        mig(2, "indexes", "-- no-transaction\nCREATE INDEX CONCURRENTLY posts_author_idx ON posts (author_id);\nCREATE INDEX CONCURRENTLY posts_created_idx ON posts (created_at);\n"),
    ];
    let report = m.run(&list).await.unwrap_or_else(|e| panic!("файл с пометкой должен выполниться вне транзакции: {e:?}"));
    assert_eq!(report.applied, vec![1, 2], "применены обе миграции");
    let valid = env.scalar("SELECT count(*) FROM pg_index WHERE indrelid = 'posts'::regclass AND indisvalid AND NOT indisprimary").await;
    assert_eq!(valid, 2, "оба индекса должны быть построены и пригодны");
    let broken = vec![list[0].clone(), list[1].clone(), mig(3, "oops", "CREATE INDEX CONCURRENTLY posts_x_idx ON posts (id)")];
    assert!(matches!(m.run(&broken).await, Err(MigrateError::Failed { version: 3, .. })), "без пометки CONCURRENTLY падает: он не работает в транзакции");
}

#[tokio::test]
async fn waits_for_the_table_by_retrying_with_a_short_lock_timeout() {
    let env = Env::new().await;
    sqlx::raw_sql("CREATE TABLE posts (id bigint PRIMARY KEY)").execute(&env.pool).await.unwrap();
    let mut holder = env.pool.begin().await.unwrap();
    sqlx::query("SELECT count(*) FROM posts").fetch_one(&mut *holder).await.unwrap();
    let m = env.migrator();
    let list = vec![mig(1, "alter", "ALTER TABLE posts ADD COLUMN lang text")];
    let job = tokio::time::timeout(Duration::from_secs(8), m.run(&list));
    let scenario = async {
        tokio::time::sleep(Duration::from_millis(700)).await;
        let reader = tokio::time::timeout(Duration::from_secs(3), sqlx::query_scalar::<_, i64>("SELECT count(*) FROM posts").fetch_one(&env.pool)).await;
        holder.rollback().await.unwrap();
        reader.is_ok()
    };
    let (report, reader_ok) = tokio::join!(job, scenario);
    assert!(reader_ok, "читатель таблицы застрял за ALTER: раннер должен ограничивать ожидание замка (lock_timeout) и повторять попытки");
    let report = report.expect("раннер не завершился после освобождения таблицы");
    assert_eq!(report.unwrap_or_else(|e| panic!("после освобождения таблицы миграция должна пройти: {e:?}")).applied, vec![1], "миграция применена с повтором");
}

#[tokio::test]
async fn gives_up_after_the_attempts_are_spent() {
    let env = Env::new().await;
    sqlx::raw_sql("CREATE TABLE posts (id bigint PRIMARY KEY)").execute(&env.pool).await.unwrap();
    let mut holder = env.pool.begin().await.unwrap();
    sqlx::query("SELECT count(*) FROM posts").fetch_one(&mut *holder).await.unwrap();
    let options = Options { attempts: 2, ..env.options() };
    let m = Migrator::new(env.pool.clone(), options);
    let list = vec![mig(1, "alter", "ALTER TABLE posts ADD COLUMN lang text")];
    let res = tokio::time::timeout(Duration::from_secs(6), m.run(&list)).await;
    holder.rollback().await.unwrap();
    let res = res.expect("две попытки по 200 мс должны закончиться раньше шести секунд");
    assert!(matches!(res, Err(MigrateError::LockTimeout { version: 1 })), "когда замок так и не получен, ожидалось LockTimeout для версии 1, а вышло {res:?}");
    assert_eq!(env.scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'lang'").await, 0, "столбец не должен появиться");
    assert_eq!(env.locks_held().await, 0, "замок раннера должен быть снят");
}
