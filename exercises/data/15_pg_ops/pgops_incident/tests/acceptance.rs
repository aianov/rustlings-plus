use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use pg_incident::{diagnose, harden_role, resolve, vacuum_bloated};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL").unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Env {
    pool: PgPool,
    options: PgConnectOptions,
    schema: String,
    tag: String,
    roles: Vec<String>,
}

impl Env {
    async fn new() -> Env {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let schema = format!("inc_{}_{}_{}", std::process::id(), n, nanos);
        let tag = format!("inc{}x{}x{}", std::process::id(), n, nanos);
        let mut conn = PgConnection::connect(&pg_url())
            .await
            .unwrap_or_else(|e| panic!("Postgres курса не отвечает ({}): {e}. Запусти docker compose up -d", pg_url()));
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\"")).execute(&mut conn).await.unwrap();
        sqlx::raw_sql("SELECT pg_advisory_lock(15012)").execute(&mut conn).await.unwrap();
        let ext = sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pgstattuple SCHEMA public").execute(&mut conn).await;
        sqlx::raw_sql("SELECT pg_advisory_unlock(15012)").execute(&mut conn).await.unwrap();
        ext.expect("расширение pgstattuple должно ставиться");
        let options = PgConnectOptions::from_str(&pg_url())
            .unwrap()
            .options([("search_path", format!("{schema},public").as_str()), ("timezone", "UTC")]);
        let pool = PgPoolOptions::new().max_connections(6).acquire_timeout(Duration::from_secs(8)).connect_with(options.clone()).await.unwrap();
        Env { pool, options, schema, tag, roles: Vec::new() }
    }

    async fn run(&self, sql: &str) {
        sqlx::raw_sql(sql).execute(&self.pool).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
    }

    async fn open(&self, suffix: &str) -> PgConnection {
        PgConnection::connect_with(&self.options.clone().application_name(&format!("{}-{suffix}", self.tag))).await.expect("соединение сеанса")
    }

    async fn wait_until(&self, what: &str, sql: &str) {
        for _ in 0..100 {
            let ok: bool = sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
            if ok {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("за 5 секунд так и не наступило: {what}");
    }

    fn track_role(&mut self, role: &str) {
        self.roles.push(role.to_string());
    }

    async fn dead(&self, table: &str) -> i64 {
        sqlx::query_scalar("SELECT dead_tuple_count FROM pgstattuple($1::regclass)").bind(table).fetch_one(&self.pool).await.expect("pgstattuple")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let (schema, tag, roles) = (self.schema.clone(), self.tag.clone(), self.roles.clone());
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            rt.block_on(async {
                if let Ok(mut conn) = PgConnection::connect(&pg_url()).await {
                    let _ = sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid()").bind(&tag).execute(&mut conn).await;
                    for role in &roles {
                        let _ = sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1").bind(role).execute(&mut conn).await;
                        let _ = sqlx::raw_sql(&format!("DROP OWNED BY \"{role}\"")).execute(&mut conn).await;
                        let _ = sqlx::raw_sql(&format!("DROP ROLE IF EXISTS \"{role}\"")).execute(&mut conn).await;
                    }
                    let _ = sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE")).execute(&mut conn).await;
                }
            });
        })
        .join();
    }
}

async fn exec(conn: &mut PgConnection, sql: &str) {
    sqlx::query(sql).execute(conn).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

struct Incident {
    env: Env,
    holder: PgConnection,
    analyst: PgConnection,
    decoy: PgConnection,
    calm: PgConnection,
    waiter: tokio::task::JoinHandle<Result<(), sqlx::Error>>,
    _writer: PgConnection,
}

async fn incident() -> Incident {
    let env = Env::new().await;
    env.run(
        "CREATE TABLE accounts (id int PRIMARY KEY, balance int NOT NULL);
         INSERT INTO accounts SELECT g, 100 FROM generate_series(1, 1000) g;
         CREATE TABLE feed (id int PRIMARY KEY, n int NOT NULL) WITH (autovacuum_enabled = false);
         INSERT INTO feed SELECT g, 0 FROM generate_series(1, 20000) g;
         CREATE TABLE settings (id int PRIMARY KEY) WITH (autovacuum_enabled = false)",
    )
    .await;
    env.run("ANALYZE accounts, feed, settings").await;
    let mut decoy = env.open("decoy").await;
    exec(&mut decoy, "BEGIN").await;
    exec(&mut decoy, "SELECT 1").await;
    let mut analyst = env.open("analyst").await;
    exec(&mut analyst, "BEGIN ISOLATION LEVEL REPEATABLE READ").await;
    exec(&mut analyst, "SELECT count(*) FROM feed").await;
    let mut calm = env.open("calm").await;
    exec(&mut calm, "SELECT 1").await;
    let mut holder = env.open("holder").await;
    exec(&mut holder, "BEGIN").await;
    exec(&mut holder, "UPDATE accounts SET balance = balance + 1 WHERE id = 1").await;
    let mut waiting = env.open("waiter").await;
    let waiter = tokio::spawn(async move { sqlx::query("UPDATE accounts SET balance = balance + 10 WHERE id = 1").execute(&mut waiting).await.map(|_| ()) });
    let mut writer = env.open("writer").await;
    exec(&mut writer, "UPDATE feed SET n = n + 1").await;
    exec(&mut writer, "UPDATE feed SET n = n + 1").await;
    exec(&mut writer, "SELECT pg_stat_force_next_flush()").await;
    exec(&mut writer, "SELECT 1").await;
    env.wait_until("ждущий встал на замок", &format!("SELECT count(*) = 1 FROM pg_stat_activity WHERE application_name = '{}-waiter' AND wait_event_type = 'Lock'", env.tag)).await;
    Incident { env, holder, analyst, decoy, calm, waiter, _writer: writer }
}

fn strip(tag: &str, names: Vec<String>) -> Vec<String> {
    names.into_iter().map(|n| n.trim_start_matches(tag).trim_start_matches('-').to_string()).collect()
}

#[tokio::test]
async fn diagnosis_names_every_kind_of_culprit() {
    let inc = incident().await;
    let d = diagnose(&inc.env.pool, &inc.env.tag).await.expect("diagnose");
    let tag = &inc.env.tag;
    assert_eq!(strip(tag, d.waiting), vec!["waiter"], "ждёт замок один сеанс");
    assert_eq!(strip(tag, d.blockers), vec!["holder"], "замок держит сеанс, обновивший строку и не закончивший транзакцию");
    assert_eq!(strip(tag, d.idle_in_tx), vec!["analyst", "decoy", "holder"], "простаивают в транзакции аналитик, второстепенный сеанс и держатель; обычное соединение и ждущий (он активен) в список не входят");
    assert_eq!(strip(tag, d.horizon_holders), vec!["analyst", "holder"], "горизонт держат аналитик (снимок) и держатель (номер транзакции на запись); сеанс, ничего не читавший и не писавший, не держит");
}

#[tokio::test]
async fn resolve_frees_the_queue_and_spares_the_innocent() {
    let mut inc = incident().await;
    let gone = resolve(&inc.env.pool, &inc.env.tag).await.expect("resolve");
    assert_eq!(strip(&inc.env.tag, gone), vec!["analyst", "holder"], "закрыть надо держателя замка и держателя горизонта, и вернуть их имена");
    let done = tokio::time::timeout(Duration::from_secs(5), inc.waiter).await.expect("ждущий должен был получить замок после закрытия держателя").expect("задача ждущего");
    assert!(done.is_ok(), "обновление ждавшего должно выполниться, а не быть закрыто");
    assert!(sqlx::query("SELECT 1").execute(&mut inc.holder).await.is_err(), "соединение держателя должно быть закрыто");
    assert!(sqlx::query("SELECT 1").execute(&mut inc.analyst).await.is_err(), "соединение аналитика должно быть закрыто");
    sqlx::query("SELECT 1").execute(&mut inc.decoy).await.expect("сеанс без снимка не мешал очистке и должен остаться жив");
    sqlx::query("SELECT 1").execute(&mut inc.calm).await.expect("обычное соединение должно остаться живо");
    let balance: i32 = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = 1").fetch_one(&inc.env.pool).await.expect("баланс");
    assert_eq!(balance, 110, "изменение закрытого держателя откатилось, изменение ждавшего применилось");
}

#[tokio::test]
async fn cleanup_works_only_after_the_horizon_is_free() {
    let inc = incident().await;
    assert!(inc.env.dead("feed").await > 10_000, "в таблице feed после двух обновлений много мёртвых версий");
    let early = vacuum_bloated(&inc.env.pool, 20).await.expect("vacuum_bloated");
    assert!(early.len() <= 1, "очищается только раздутая таблица: {early:?}");
    assert!(inc.env.dead("feed").await > 10_000, "пока аналитик держит снимок, версии убрать нельзя");
    resolve(&inc.env.pool, &inc.env.tag).await.expect("resolve");
    let mut handled = Vec::new();
    for _ in 0..40 {
        sqlx::query("SELECT pg_stat_clear_snapshot()").execute(&inc.env.pool).await.expect("снимок статистики");
        handled = vacuum_bloated(&inc.env.pool, 20).await.expect("vacuum_bloated");
        if !handled.is_empty() && inc.env.dead("feed").await == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(handled, vec!["feed"], "в отчёт попадает только feed: accounts и пустая settings не раздуты");
    assert_eq!(inc.env.dead("feed").await, 0, "после закрытия держателей VACUUM должен убрать все мёртвые версии");
}

#[tokio::test]
async fn hardened_role_has_limits_and_least_privileges() {
    let mut env = Env::new().await;
    env.run("CREATE TABLE posts (id bigserial PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('первый')").await;
    let role = format!("inc_app_{}", env.tag);
    env.track_role(&role);
    harden_role(&env.pool, &role, "pw_inc_secret").await.expect("harden_role");
    let options = PgConnectOptions::from_str(&pg_url()).unwrap().username(&role).password("pw_inc_secret").options([("search_path", env.schema.as_str())]);
    let mut c = PgConnection::connect_with(&options).await.unwrap_or_else(|e| panic!("роль должна входить по паролю: {e}"));
    let st: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&mut c).await.expect("SHOW");
    let idle: String = sqlx::query_scalar("SHOW idle_in_transaction_session_timeout").fetch_one(&mut c).await.expect("SHOW");
    assert_eq!((st.as_str(), idle.as_str()), ("1s", "2s"), "ограничения роли: statement_timeout 1s, idle_in_transaction_session_timeout 2s");
    let started = std::time::Instant::now();
    assert!(sqlx::query("SELECT pg_sleep(5)").execute(&mut c).await.is_err(), "долгий запрос должен прерываться");
    assert!(started.elapsed() < Duration::from_secs(3), "прерывание должно случиться примерно через секунду");
    let id: i64 = sqlx::query_scalar("INSERT INTO posts (body) VALUES ('второй') RETURNING id").fetch_one(&mut c).await.unwrap_or_else(|e| panic!("запись в таблицу с bigserial: {e}"));
    assert_eq!(id, 2, "второй пост");
    for sql in ["CREATE TABLE evil (id int)", "DROP TABLE posts", "TRUNCATE posts"] {
        let err = sqlx::query(sql).execute(&mut c).await.expect_err(sql);
        let code = match &err {
            sqlx::Error::Database(d) => d.code().map(|c| c.to_string()).unwrap_or_default(),
            _ => String::new(),
        };
        assert_eq!(code, "42501", "{sql}: ожидалась ошибка прав, а пришло {err}");
    }
    env.run("CREATE TABLE comments (id bigserial PRIMARY KEY, body text NOT NULL)").await;
    exec(&mut c, "INSERT INTO comments (body) VALUES ('привет')").await;
}
