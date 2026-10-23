fn services_schema(p: &str) -> String {
    format!(
        "CREATE SCHEMA {p}_auth;
         CREATE SCHEMA {p}_profiles;
         CREATE TABLE {p}_auth.accounts (
             id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
             email text NOT NULL UNIQUE,
             password_hash text NOT NULL
         );
         CREATE TABLE {p}_profiles.profiles (
             account_id uuid PRIMARY KEY,
             display_name text NOT NULL,
             bio text
         )"
    )
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn expect_error(db: &TestDb, sql: &str, code: &str, why: &str) {
        let err = db
            .fails(sql)
            .await
            .unwrap_or_else(|| panic!("{why}: Postgres принял команду, а должен был отвергнуть:\n{sql}"));
        assert_eq!(err.code, code, "{why}: ошибка не та, что ждали (SQLSTATE {code}). База сказала: {}", err.message);
    }
    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    async fn prepared() -> (TestDb, String) {
        let mut db = TestDb::new().await;
        let p = db.schema().to_string();
        db.track_schema(&format!("{p}_auth"));
        db.track_schema(&format!("{p}_profiles"));
        db.run(&services_schema(&p)).await;
        (db, p)
    }

    #[tokio::test]
    async fn accounts_rules() {
        let (db, p) = prepared().await;
        db.run(&format!("INSERT INTO {p}_auth.accounts (email, password_hash) VALUES ('a@x.org', 'h1')")).await;
        assert_eq!(cell(&db, &format!("SELECT length(id::text) FROM {p}_auth.accounts")).await, "36", "id аккаунта это uuid, который база выдаёт сама");
        expect_error(&db, &format!("INSERT INTO {p}_auth.accounts (email, password_hash) VALUES ('a@x.org', 'h2')"), "23505", "повторная почта").await;
        expect_error(&db, &format!("INSERT INTO {p}_auth.accounts (email, password_hash) VALUES ('b@x.org', NULL)"), "23502", "хеш пароля обязателен").await;
    }

    #[tokio::test]
    async fn profiles_do_not_depend_on_accounts() {
        let (db, p) = prepared().await;
        db.run(&format!("INSERT INTO {p}_profiles.profiles (account_id, display_name) VALUES ('11111111-1111-4111-8111-111111111111', 'Ghost')")).await;
        assert_eq!(cell(&db, &format!("SELECT display_name FROM {p}_profiles.profiles")).await, "Ghost", "профиль для аккаунта, которого нет в этой схеме, должен вставляться: связь между сервисами не проверяется базой");
        expect_error(&db, &format!("INSERT INTO {p}_profiles.profiles (account_id, display_name) VALUES ('11111111-1111-4111-8111-111111111111', 'Twin')"), "23505", "второй профиль того же аккаунта").await;
        expect_error(&db, &format!("INSERT INTO {p}_profiles.profiles (account_id, display_name) VALUES (gen_random_uuid(), NULL)"), "23502", "имя обязательно").await;
        let fks = cell(&db, &format!("SELECT count(*) FROM pg_constraint c JOIN pg_namespace n ON n.oid = c.connamespace WHERE c.contype = 'f' AND n.nspname LIKE '{p}%'")).await;
        assert_eq!(fks, "0", "между сервисами не должно быть внешних ключей");
    }
}
