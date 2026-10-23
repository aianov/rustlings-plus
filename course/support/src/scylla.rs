//! Помощники для тестов со ScyllaDB (фича `scylla`).
//!
//! Каждый тест получает собственный keyspace с уникальным именем (`SimpleStrategy`, одна реплика):
//! тесты идут параллельно и не видят таблиц друг друга. Сессия уже переключена на этот keyspace,
//! поэтому в запросах таблицы пишутся без префикса. Keyspace удаляется (`DROP KEYSPACE`), когда
//! `TestKs` уничтожается, в том числе после паники в `assert!`.
//!
//! API:
//!
//! - `uri()`, `DEFAULT_URI`: адрес из `RUSTLINGS_SCYLLA_URI` или значение по умолчанию.
//! - `session().await -> Session`: новая сессия без keyspace (паника с понятным текстом, если
//!   Scylla не отвечает). Нужна задачам, где ученик сам создаёт keyspace.
//! - `TestKs::new().await`: свой keyspace и сессия `session` на него.
//! - `TestKs::with_cql(script).await`: то же, но сразу выполняет CQL-скрипт. В скрипте может быть
//!   много команд через `;` (разделитель внутри строк в одинарных кавычках не считается).
//! - `ks.run(script).await`: выполнить ещё один скрипт, паника с текстом ошибки при сбое.
//! - `ks.table(cql).await -> Table`: выполнить запрос и получить результат как текст. Паника с
//!   понятным сообщением, если Scylla отвергла запрос. `ks.try_table(cql).await -> Result<Table, CqlError>`
//!   то же без паники.
//! - `ks.fails(script).await -> Option<CqlError>`: выполнить скрипт и вернуть ошибку, если она
//!   случилась (`None`, если всё принято). `CqlError { kind, message }`: `kind` это имя ошибки базы
//!   (`Invalid`, `SyntaxError`, `AlreadyExists`, `Unauthorized`, ...), `message` её текст.
//! - `ks.name() -> &str`: имя keyspace теста. Задача, где ученик создаёт keyspace, получает от теста имя
//!   и обязана его использовать; `ks.track_keyspace(name)` записывает ещё один keyspace, который тоже
//!   надо удалить в конце.
//! - `Table { columns, rows }`: значения в тексте. `NULL` выводится строкой `"NULL"`, булевы значения
//!   как `true` и `false`, `timestamp` как число миллисекунд, `uuid` и `timeuuid` как обычно, список как
//!   `[a, b]`, множество как `{a, b}`, словарь как `{k: v}`, UDT как `{поле: значение}`. Методы: `len()`,
//!   `is_empty()`, `cell(row, col)`, `column(index)`, `col("имя")`, `sorted()`. `Display` печатает
//!   таблицу для сообщений assert.

use std::fmt;

use scylla::frame::response::result::CqlValue;
use scylla::transport::errors::{DbError, QueryError};
use scylla::{Session, SessionBuilder};

pub const DEFAULT_URI: &str = "127.0.0.1:59042";

pub fn uri() -> String {
    crate::env_or("RUSTLINGS_SCYLLA_URI", DEFAULT_URI)
}

pub async fn session() -> Session {
    SessionBuilder::new()
        .known_node(uri())
        .build()
        .await
        .unwrap_or_else(|e| panic!("Scylla курса не отвечает по адресу {}: {e}. Запусти `docker compose up -d`.", uri()))
}

pub struct TestKs {
    pub session: Session,
    keyspace: String,
    extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CqlError {
    pub kind: String,
    pub message: String,
}

impl fmt::Display for CqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

fn to_error(e: QueryError) -> CqlError {
    match e {
        QueryError::DbError(db, message) => {
            let dbg = format!("{db:?}");
            let kind = dbg.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("").to_string();
            let kind = if matches!(db, DbError::Other(_)) { "Other".to_string() } else { kind };
            CqlError { kind, message }
        }
        other => CqlError { kind: "Driver".to_string(), message: other.to_string() },
    }
}

pub fn split_statements(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in script.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                cur.push(ch);
            }
            ';' if !quoted => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

impl TestKs {
    pub async fn new() -> TestKs {
        let keyspace = crate::unique_name("t");
        let session = session().await;
        session
            .query(
                format!("CREATE KEYSPACE \"{keyspace}\" WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}"),
                &[],
            )
            .await
            .unwrap_or_else(|e| panic!("не удалось создать keyspace теста: {e}"));
        session
            .use_keyspace(&keyspace, true)
            .await
            .unwrap_or_else(|e| panic!("не удалось переключиться на keyspace теста: {e}"));
        TestKs { session, keyspace, extra: Vec::new() }
    }

    pub async fn with_cql(script: &str) -> TestKs {
        let ks = TestKs::new().await;
        ks.run(script).await;
        ks
    }

    pub async fn run(&self, script: &str) {
        for stmt in split_statements(script) {
            if let Err(e) = self.session.query(stmt.as_str(), &[]).await {
                panic!("Scylla отвергла команду `{stmt}`: {}", to_error(e));
            }
        }
    }

    pub async fn try_table(&self, cql: &str) -> Result<Table, CqlError> {
        let res = self.session.query(cql.trim().trim_end_matches(';'), &[]).await.map_err(to_error)?;
        let columns = res.col_specs.iter().map(|c| c.name.clone()).collect();
        let rows = res
            .rows
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.columns.iter().map(|c| c.as_ref().map(show).unwrap_or_else(|| "NULL".to_string())).collect())
            .collect();
        Ok(Table { columns, rows })
    }

    pub async fn table(&self, cql: &str) -> Table {
        self.try_table(cql).await.unwrap_or_else(|e| panic!("Scylla отвергла запрос `{cql}`: {e}"))
    }

    pub async fn fails(&self, script: &str) -> Option<CqlError> {
        for stmt in split_statements(script) {
            if let Err(e) = self.session.query(stmt.as_str(), &[]).await {
                return Some(to_error(e));
            }
        }
        None
    }

    pub fn name(&self) -> &str {
        &self.keyspace
    }

    pub fn track_keyspace(&mut self, name: &str) {
        self.extra.push(name.to_string());
    }
}

impl Drop for TestKs {
    fn drop(&mut self) {
        let mut names = std::mem::take(&mut self.extra);
        names.push(std::mem::take(&mut self.keyspace));
        names.retain(|s| !s.is_empty());
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                return;
            };
            rt.block_on(async {
                if let Ok(session) = SessionBuilder::new().known_node(uri()).build().await {
                    for name in &names {
                        let _ = session.query(format!("DROP KEYSPACE IF EXISTS \"{name}\""), &[]).await;
                    }
                }
            });
        })
        .join();
    }
}

fn show(v: &CqlValue) -> String {
    match v {
        CqlValue::Ascii(s) | CqlValue::Text(s) => s.clone(),
        CqlValue::Boolean(b) => b.to_string(),
        CqlValue::Int(x) => x.to_string(),
        CqlValue::BigInt(x) => x.to_string(),
        CqlValue::SmallInt(x) => x.to_string(),
        CqlValue::TinyInt(x) => x.to_string(),
        CqlValue::Counter(c) => c.0.to_string(),
        CqlValue::Double(x) => x.to_string(),
        CqlValue::Float(x) => x.to_string(),
        CqlValue::Timestamp(t) => t.0.to_string(),
        CqlValue::Uuid(u) => u.to_string(),
        CqlValue::Timeuuid(u) => u.to_string(),
        CqlValue::Inet(ip) => ip.to_string(),
        CqlValue::List(items) => format!("[{}]", items.iter().map(show).collect::<Vec<_>>().join(", ")),
        CqlValue::Set(items) => format!("{{{}}}", items.iter().map(show).collect::<Vec<_>>().join(", ")),
        CqlValue::Map(pairs) => format!("{{{}}}", pairs.iter().map(|(k, v)| format!("{}: {}", show(k), show(v))).collect::<Vec<_>>().join(", ")),
        CqlValue::UserDefinedType { fields, .. } => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, v)| format!("{name}: {}", v.as_ref().map(show).unwrap_or_else(|| "NULL".to_string())))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        CqlValue::Tuple(items) => format!(
            "({})",
            items.iter().map(|v| v.as_ref().map(show).unwrap_or_else(|| "NULL".to_string())).collect::<Vec<_>>().join(", ")
        ),
        other => format!("{other:?}"),
    }
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
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.chars().count()).collect();
        for r in &self.rows {
            for (i, c) in r.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(c.chars().count());
                }
            }
        }
        let line = |cells: &[String]| {
            cells.iter().enumerate().map(|(i, c)| format!(" {c:<w$} ", w = widths.get(i).copied().unwrap_or(0))).collect::<Vec<_>>().join("|")
        };
        writeln!(f, "{}", line(&self.columns))?;
        writeln!(f, "{}", widths.iter().map(|w| "-".repeat(w + 2)).collect::<Vec<_>>().join("+"))?;
        for r in &self.rows {
            writeln!(f, "{}", line(r))?;
        }
        write!(f, "({} строк)", self.rows.len())
    }
}
