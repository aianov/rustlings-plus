// В проде уже лежит таблица members с данными (см. тест). Команду «удалить и создать заново»
// не предлагать: данные пользователей терять нельзя. Нужно привести таблицу к новому виду
// на месте:
//   - таблица называется users, а не members;
//   - столбец name называется username, столбец mail называется email;
//   - устаревшего столбца legacy_flag больше нет;
//   - joined хранит даты строками вроде '2024-05-17', а должен быть настоящим типом date;
//   - добавлен необязательный текстовый bio;
//   - добавлен обязательный status, у существующих строк он должен стать 'active';
//   - почта уникальна, ограничение называется users_email_uniq.
// TODO: Напиши миграцию (несколько команд ALTER TABLE).

fn migration() -> &'static str {
    ""
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    async fn migrated() -> TestDb {
        let db = TestDb::new().await;
        db.run(
            "CREATE TABLE members (
                 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                 name text NOT NULL,
                 mail text NOT NULL,
                 joined text NOT NULL,
                 legacy_flag boolean
             );
             INSERT INTO members (name, mail, joined, legacy_flag) VALUES ('alice', 'alice@example.com', '2024-05-17', true), ('bob', 'bob@example.com', '2023-12-01', false)",
        )
        .await;
        db.run(migration()).await;
        db
    }

    #[tokio::test]
    async fn data_survives() {
        let db = migrated().await;
        let t = db.table("SELECT id, username, email, status, bio, joined FROM users ORDER BY id").await;
        assert_eq!(
            t.rows,
            [["1", "alice", "alice@example.com", "active", "NULL", "2024-05-17"], ["2", "bob", "bob@example.com", "active", "NULL", "2023-12-01"]],
            "данные должны сохраниться, у старых строк status = active:\n{t}"
        );
    }

    #[tokio::test]
    async fn old_shape_is_gone() {
        let db = migrated().await;
        let t = db
            .table(
                "SELECT column_name FROM information_schema.columns
                 WHERE table_schema = current_schema() AND table_name = 'users' ORDER BY column_name",
            )
            .await;
        assert_eq!(t.column(0), ["bio", "email", "id", "joined", "status", "username"], "набор столбцов после миграции:\n{t}");
        assert_eq!(cell(&db, "SELECT to_regclass('members') IS NULL").await, "true", "старой таблицы members быть не должно");
    }

    #[tokio::test]
    async fn joined_is_a_real_date() {
        let db = migrated().await;
        let t = db.table("SELECT data_type FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'users' AND column_name = 'joined'").await;
        assert_eq!(t.rows, [["date"]], "joined должен стать типом date, а значения сохраниться:\n{t}");
        assert_eq!(cell(&db, "SELECT count(*) FROM users WHERE joined > date '2024-01-01'").await, "1", "сравнение с датой должно работать: в 2024 и позже зарегистрирована одна alice");
    }

    #[tokio::test]
    async fn email_unique_and_status_default() {
        let db = migrated().await;
        let err = db.fails("INSERT INTO users (username, email, joined) VALUES ('x', 'alice@example.com', '2025-01-01')").await.expect("почта должна быть уникальной");
        assert_eq!(err.constraint.as_deref(), Some("users_email_uniq"), "ограничение называется users_email_uniq. База: {}", err.message);
        db.run("INSERT INTO users (username, email, joined) VALUES ('carol', 'carol@example.com', '2025-01-01')").await;
        assert_eq!(cell(&db, "SELECT status FROM users WHERE username = 'carol'").await, "active", "status по умолчанию active");
    }
}
