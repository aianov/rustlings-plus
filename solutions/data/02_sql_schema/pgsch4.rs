fn api_keys_table() -> &'static str {
    "CREATE TABLE api_keys (
        id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
        owner text NOT NULL
    )"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn database_generates_random_ids() {
        let db = TestDb::new().await;
        db.run(api_keys_table()).await;
        db.run("INSERT INTO api_keys (owner) VALUES ('alice'), ('bob')").await;
        let t = db.table("SELECT id::text FROM api_keys").await;
        let ids = t.column(0);
        assert_ne!(ids[0], ids[1], "ключи должны быть разными");
        assert_eq!(cell(&db, "SELECT count(*) FROM api_keys WHERE substr(id::text, 15, 1) = '4'").await, "2", "ждём случайные uuid версии 4:\n{t}");
    }

    #[tokio::test]
    async fn explicit_id_is_accepted() {
        let db = TestDb::new().await;
        db.run(api_keys_table()).await;
        db.run("INSERT INTO api_keys (id, owner) VALUES ('11111111-1111-4111-8111-111111111111', 'alice')").await;
        assert_eq!(cell(&db, "SELECT id::text FROM api_keys").await, "11111111-1111-4111-8111-111111111111");
    }
}
