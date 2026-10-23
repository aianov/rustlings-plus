//! Помощники для тестов с PostgreSQL (фича `pg`).
//!
//! Каждый тест получает собственную схему с уникальным именем: тесты идут параллельно и не видят
//! данных друг друга. Все соединения пула настроены так, что `search_path` указывает на эту схему,
//! а `timezone` равен UTC, поэтому в запросах таблицы пишутся без префикса, а времена выводятся
//! одинаково на любой машине. Схема удаляется (`DROP SCHEMA ... CASCADE`), когда `TestDb`
//! уничтожается, в том числе после паники в `assert!`.
//!
//! API:
//!
//! - `url()`, `DEFAULT_URL`: адрес из `RUSTLINGS_PG_URL` или значение по умолчанию.
//! - `TestDb::new().await`: пустая схема и пул `pool` на неё (до 8 соединений).
//! - `TestDb::with_sql(script).await`: то же, но сразу выполняет SQL-скрипт (схема и данные).
//!   В скрипте может быть много команд через `;`.
//! - `db.run(script).await`: выполнить ещё один скрипт, паника с текстом ошибки базы при сбое.
//! - `db.table(sql).await -> Table`: выполнить запрос и получить результат как текст. Паника
//!   с понятным сообщением, если база отвергла запрос. Параметров `$1` нет (для них берите
//!   `db.pool` и обычный `sqlx::query(...).bind(...)`).
//! - `db.try_table(sql).await -> Result<Table, String>`: то же без паники, `Err` содержит
//!   сообщение базы.
//! - `db.fails(script).await -> Option<DbError>`: выполнить скрипт и вернуть ошибку базы, если она
//!   случилась (`None`, если Postgres скрипт принял). `DbError { code, constraint, message }`:
//!   `code` это SQLSTATE (например `23505`), `constraint` имя нарушенного ограничения.
//! - `db.track_schema(name)`: ещё одна схема, созданная тестом, которую тоже надо удалить в конце.
//! - `db.schema() -> &str`: имя схемы теста; `db.close().await`: закрыть пул (схема удалится
//!   при уничтожении значения).
//! - `db.options() -> PgConnectOptions`: параметры подключения к схеме теста (search_path, UTC), чтобы
//!   тест мог собрать свой пул (`PgPoolOptions::new().max_connections(1)...connect_with(db.options())`).
//! - `db.counted_pool(max).await -> (PgPool, Counter)`: пул через прокси, который считает выполненные
//!   запросы (сообщения Bind и Query протокола). `counter.get()` и `counter.reset()`. Так тест
//!   ловит N+1 детерминированно, без замеров времени.
//! - `StreamExt`, `TryStreamExt` реэкспортированы из `futures`: нужны, чтобы читать `fetch()` потоком.
//! - `Table { columns, rows }`: значения в тексте как их показывает `psql`; `NULL` выводится
//!   строкой `"NULL"`, булевы значения как `true` и `false`. Методы: `len()`, `is_empty()`,
//!   `cell(row, col)`, `column(index)`, `col("имя")`, `sorted()` (строки по возрастанию, для
//!   запросов без ORDER BY). `Display` печатает таблицу для сообщений assert.

use std::fmt;

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub use futures::{StreamExt, TryStreamExt};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgRow, PgSslMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use sqlx::{Column, Connection, PgConnection, Row, TypeInfo, ValueRef};

pub const DEFAULT_URL: &str = "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings";

pub fn url() -> String {
    crate::env_or("RUSTLINGS_PG_URL", DEFAULT_URL)
}

pub struct TestDb {
    pub pool: PgPool,
    schema: String,
    extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbError {
    pub code: String,
    pub constraint: Option<String>,
    pub message: String,
}

impl TestDb {
    pub async fn new() -> TestDb {
        let schema = crate::unique_name("t");
        let mut conn = PgConnection::connect(&url()).await.unwrap_or_else(|e| {
            panic!("Postgres курса не отвечает по адресу {}: {e}. Запусти `docker compose up -d`.", url())
        });
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\""))
            .execute(&mut conn)
            .await
            .unwrap_or_else(|e| panic!("не удалось создать схему теста: {e}"));
        let setup = format!("SET search_path TO \"{schema}\", public; SET timezone TO 'UTC'");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(8))
            .after_connect(move |conn, _| {
                let setup = setup.clone();
                Box::pin(async move {
                    sqlx::Executor::execute(&mut *conn, setup.as_str()).await?;
                    Ok(())
                })
            })
            .connect(&url())
            .await
            .unwrap_or_else(|e| panic!("не удалось открыть пул соединений: {e}"));
        TestDb { pool, schema, extra: Vec::new() }
    }

    pub async fn with_sql(script: &str) -> TestDb {
        let db = TestDb::new().await;
        db.run(script).await;
        db
    }

    pub async fn run(&self, script: &str) {
        if let Err(e) = sqlx::raw_sql(script).execute(&self.pool).await {
            panic!("Postgres отверг SQL-скрипт: {e}");
        }
    }

    pub async fn try_table(&self, sql: &str) -> Result<Table, String> {
        let rows = sqlx::raw_sql(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        let columns = match rows.first() {
            Some(row) => row.columns().iter().map(|c| c.name().to_string()).collect(),
            None => self.describe_columns(sql).await,
        };
        let rows = rows.iter().map(row_to_strings).collect();
        Ok(Table { columns, rows })
    }

    pub async fn table(&self, sql: &str) -> Table {
        match self.try_table(sql).await {
            Ok(t) => t,
            Err(e) => panic!("Postgres отверг запрос:\n{sql}\nОшибка базы: {e}"),
        }
    }

    pub async fn fails(&self, script: &str) -> Option<DbError> {
        match sqlx::raw_sql(script).execute(&self.pool).await {
            Ok(_) => None,
            Err(sqlx::Error::Database(e)) => Some(DbError {
                code: e.code().map(|c| c.to_string()).unwrap_or_default(),
                constraint: e.constraint().map(String::from),
                message: e.message().to_string(),
            }),
            Err(e) => Some(DbError { code: String::new(), constraint: None, message: e.to_string() }),
        }
    }

    pub fn track_schema(&mut self, name: &str) {
        self.extra.push(name.to_string());
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn options(&self) -> PgConnectOptions {
        PgConnectOptions::from_str(&url())
            .unwrap_or_else(|e| panic!("неверный адрес Postgres {}: {e}", url()))
            .options([("search_path", format!("{},public", self.schema)), ("timezone", "UTC".to_string())])
    }

    pub async fn counted_pool(&self, max: u32) -> (PgPool, Counter) {
        let base = PgConnectOptions::from_str(&url()).unwrap_or_else(|e| panic!("неверный адрес Postgres {}: {e}", url()));
        let target = (base.get_host().to_string(), base.get_port());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("прокси не смог занять порт");
        let port = listener.local_addr().expect("адрес прокси").port();
        let counter = Counter(Arc::new(AtomicUsize::new(0)));
        let seen = counter.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let seen = seen.clone();
                let target = target.clone();
                tokio::spawn(async move {
                    let Ok(server) = TcpStream::connect((target.0.as_str(), target.1)).await else {
                        return;
                    };
                    let (mut cr, mut cw) = client.into_split();
                    let (mut sr, mut sw) = server.into_split();
                    let down = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut sr, &mut cw).await;
                        let _ = cw.shutdown().await;
                    });
                    let mut parser = Frames::new();
                    let mut buf = vec![0u8; 16 * 1024];
                    while let Ok(n) = cr.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        parser.feed(&buf[..n], &seen);
                        if sw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    let _ = sw.shutdown().await;
                    down.abort();
                });
            }
        });
        let options = self.options().host("127.0.0.1").port(port).ssl_mode(PgSslMode::Disable);
        let pool = PgPoolOptions::new()
            .max_connections(max)
            .acquire_timeout(std::time::Duration::from_secs(8))
            .connect_with(options)
            .await
            .unwrap_or_else(|e| panic!("не удалось открыть пул через прокси: {e}"));
        counter.reset();
        (pool, counter)
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    async fn describe_columns(&self, sql: &str) -> Vec<String> {
        use sqlx::Executor;
        match (&self.pool).describe(sql.trim().trim_end_matches(';')).await {
            Ok(d) => d.columns().iter().map(|c| c.name().to_string()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let mut schemas = std::mem::take(&mut self.extra);
        schemas.push(std::mem::take(&mut self.schema));
        schemas.retain(|s| !s.is_empty());
        if schemas.is_empty() {
            return;
        }
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                return;
            };
            rt.block_on(async {
                if let Ok(mut conn) = PgConnection::connect(&url()).await {
                    for schema in &schemas {
                        let _ = sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))
                            .execute(&mut conn)
                            .await;
                    }
                }
            });
        })
        .join();
    }
}

#[derive(Clone)]
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.0.store(0, Ordering::SeqCst);
    }
}

struct Frames {
    started: bool,
    buf: Vec<u8>,
}

impl Frames {
    fn new() -> Frames {
        Frames { started: false, buf: Vec::new() }
    }

    fn feed(&mut self, data: &[u8], count: &Counter) {
        self.buf.extend_from_slice(data);
        loop {
            if !self.started {
                if self.buf.len() < 4 {
                    return;
                }
                let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
                if self.buf.len() < len {
                    return;
                }
                self.buf.drain(..len);
                self.started = true;
                continue;
            }
            if self.buf.len() < 5 {
                return;
            }
            let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
            if self.buf.len() < 1 + len {
                return;
            }
            if self.buf[0] == b'B' || self.buf[0] == b'Q' {
                count.0.fetch_add(1, Ordering::SeqCst);
            }
            self.buf.drain(..1 + len);
        }
    }
}

fn row_to_strings(row: &PgRow) -> Vec<String> {
    (0..row.columns().len())
        .map(|i| {
            let Ok(raw) = row.try_get_raw(i) else {
                return "?".to_string();
            };
            if raw.is_null() {
                return "NULL".to_string();
            }
            let text = sqlx::postgres::PgValueRef::as_str(&raw).unwrap_or("?").to_string();
            if row.column(i).type_info().name() == "BOOL" {
                return if text == "t" { "true".to_string() } else { "false".to_string() };
            }
            text
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn cell(&self, row: usize, col: usize) -> &str {
        &self.rows[row][col]
    }

    pub fn column(&self, index: usize) -> Vec<&str> {
        self.rows.iter().map(|r| r[index].as_str()).collect()
    }

    pub fn col(&self, name: &str) -> Vec<&str> {
        match self.columns.iter().position(|c| c == name) {
            Some(i) => self.column(i),
            None => panic!("в результате нет столбца `{name}`, есть: {}", self.columns.join(", ")),
        }
    }

    pub fn sorted(mut self) -> Table {
        self.rows.sort();
        self
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.columns.join(" | "))?;
        for row in self.rows.iter().take(15) {
            writeln!(f, "{}", row.join(" | "))?;
        }
        if self.rows.len() > 15 {
            writeln!(f, "... и ещё {} строк", self.rows.len() - 15)?;
        }
        write!(f, "({} строк)", self.rows.len())
    }
}
