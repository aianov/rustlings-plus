// Регистрация проверяет, что ник свободен, но зарегистрировались и Alice, и alice: в профиле это
// два разных человека с почти одинаковыми именами, а в упоминаниях `@alice` неоднозначно.
// Ник должен быть уникален без учёта регистра, но показываться так, как его ввёл человек
// (Alice остаётся Alice). Таблица users (id, username).
// TODO: Допиши схему так, чтобы второй ник отвергался.

fn schema() -> &'static str {
    "CREATE TABLE users (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         username text NOT NULL UNIQUE
     )"
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

    #[tokio::test]
    async fn case_variants_collide() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('Alice')").await;
        expect_error(&db, "INSERT INTO users (username) VALUES ('alice')", "23505", "тот же ник другим регистром").await;
        expect_error(&db, "INSERT INTO users (username) VALUES ('ALICE')", "23505", "тот же ник капсом").await;
        db.run("INSERT INTO users (username) VALUES ('alice2'), ('bob')").await;
    }

    #[tokio::test]
    async fn display_case_is_preserved() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('Alice')").await;
        assert_eq!(cell(&db, "SELECT username FROM users WHERE id = 1").await, "Alice", "ник должен храниться так, как введён (Alice), а не приведённым к нижнему регистру");
        assert_eq!(cell(&db, "SELECT id FROM users WHERE lower(username) = lower('aLiCe')").await, "1", "поиск без учёта регистра находит пользователя");
    }
}
