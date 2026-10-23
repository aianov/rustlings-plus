fn cleanup() -> &'static str {
    "DROP TABLE IF EXISTS old_sessions;
     DROP TABLE IF EXISTS tmp_import;
     TRUNCATE logs RESTART IDENTITY"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(
            "CREATE TABLE old_sessions (id integer);
             CREATE TABLE logs (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, line text);
             INSERT INTO logs (line) VALUES ('a'), ('b'), ('c')",
        )
        .await;
        db
    }

    #[tokio::test]
    async fn removes_and_truncates() {
        let db = prepared().await;
        db.run(cleanup()).await;
        assert_eq!(cell(&db, "SELECT to_regclass('old_sessions') IS NULL").await, "true", "old_sessions должна быть удалена");
        assert_eq!(cell(&db, "SELECT count(*) FROM logs").await, "0", "logs должна остаться, но пустая");
        db.run("INSERT INTO logs (line) VALUES ('fresh')").await;
        assert_eq!(cell(&db, "SELECT id FROM logs").await, "1", "нумерация должна начаться заново с 1");
    }

    #[tokio::test]
    async fn safe_to_run_twice() {
        let db = prepared().await;
        db.run(cleanup()).await;
        db.run(cleanup()).await;
        assert_eq!(cell(&db, "SELECT count(*) FROM logs").await, "0");
    }
}
