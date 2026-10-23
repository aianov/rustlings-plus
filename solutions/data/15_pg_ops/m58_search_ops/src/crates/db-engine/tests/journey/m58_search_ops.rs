use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema};

const APP_PASSWORD: &str = "app_secret";

fn owner_url() -> String {
    std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string())
}

fn app_url(role: &str) -> String {
    let owner = owner_url();
    let (_, rest) = owner.split_once('@').expect("адрес с пользователем");
    format!("postgres://{role}:{APP_PASSWORD}@{rest}")
}

#[tokio::test]
async fn app_role_works_without_ddl_and_with_timeouts() {
    let id = std::process::id();
    let role = format!("t_app_{id}");
    let config = DbConfig::new(&owner_url(), &format!("t_roles_{id}")).pool(2);
    let _ = drop_schema(&config).await;
    let owner = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    let setup = format!(
        "DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{role}') THEN
                CREATE ROLE {role} LOGIN PASSWORD '{APP_PASSWORD}';
            END IF;
        END $$;
        ALTER ROLE {role} SET statement_timeout = '1500ms';
        ALTER ROLE {role} SET idle_in_transaction_session_timeout = '5s';
        CREATE TABLE pets (id BIGINT PRIMARY KEY, name TEXT NOT NULL);
        CREATE FUNCTION make_room() RETURNS void
            LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS
            'BEGIN CREATE TABLE IF NOT EXISTS rooms (id BIGINT); END';
        GRANT USAGE ON SCHEMA {schema} TO {role};
        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA {schema} TO {role};
        GRANT EXECUTE ON FUNCTION make_room() TO {role};",
        schema = config.schema
    );
    sqlx::raw_sql(&setup)
        .execute(&owner)
        .await
        .expect("роль и права");
    let app = connect(&DbConfig::new(&app_url(&role), &config.schema).pool(1))
        .await
        .expect("роль приложения входит в свою схему");
    sqlx::query("INSERT INTO pets VALUES (1, 'rex')")
        .execute(&app)
        .await
        .expect("данные можно");
    let ddl = sqlx::query("CREATE TABLE intruders (id BIGINT)")
        .execute(&app)
        .await;
    let code = ddl.err().and_then(|error| {
        error
            .as_database_error()
            .and_then(|db| db.code().map(|c| c.to_string()))
    });
    assert_eq!(
        code.as_deref(),
        Some("42501"),
        "DDL ролью приложения нельзя"
    );
    sqlx::query("SELECT make_room()")
        .execute(&app)
        .await
        .expect("обслуживание через SECURITY DEFINER");
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&app)
        .await
        .expect("настройка");
    assert_eq!(timeout, "1500ms", "statement_timeout висит на роли");
    app.close().await;
    sqlx::raw_sql(&format!(
        "DROP SCHEMA {} CASCADE; DROP OWNED BY {role}; DROP ROLE {role};",
        config.schema
    ))
    .execute(&owner)
    .await
    .expect("уборка");
}
