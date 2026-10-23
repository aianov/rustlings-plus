fn tags_table() -> &'static str {
    "CREATE TABLE tags (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        name text NOT NULL
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
    async fn ids_are_given_by_database() {
        let db = TestDb::new().await;
        db.run(tags_table()).await;
        db.run("INSERT INTO tags (name) VALUES ('rust'), ('sql')").await;
        let t = db.table("SELECT id FROM tags ORDER BY id").await;
        assert_eq!(t.column(0), ["1", "2"], "база должна выдавать номера 1 и 2 сама:\n{t}");
    }

    #[tokio::test]
    async fn application_cannot_override_id() {
        let db = TestDb::new().await;
        db.run(tags_table()).await;
        expect_error(&db, "INSERT INTO tags (id, name) VALUES (100, 'rust')", "428C9", "явный номер при ALWAYS").await;
    }

    #[tokio::test]
    async fn id_is_primary_key() {
        let db = TestDb::new().await;
        db.run(tags_table()).await;
        let n = cell(
            &db,
            "SELECT count(*) FROM information_schema.table_constraints
             WHERE table_schema = current_schema() AND table_name = 'tags' AND constraint_type = 'PRIMARY KEY'",
        )
        .await;
        assert_eq!(n, "1", "у tags должен быть первичный ключ");
    }
}
