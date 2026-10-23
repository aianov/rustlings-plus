fn users_table() -> &'static str {
    "CREATE TABLE users (
        id bigint,
        username text,
        email text,
        created_at timestamptz
    )"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    #[tokio::test]
    async fn table_has_expected_columns() {
        let db = TestDb::new().await;
        db.run(users_table()).await;
        let t = db
            .table(
                "SELECT column_name, data_type FROM information_schema.columns
                 WHERE table_schema = current_schema() AND table_name = 'users'
                 ORDER BY ordinal_position",
            )
            .await;
        let want = [
            ["id", "bigint"],
            ["username", "text"],
            ["email", "text"],
            ["created_at", "timestamp with time zone"],
        ];
        let got: Vec<[&str; 2]> = t.rows.iter().map(|r| [r[0].as_str(), r[1].as_str()]).collect();
        assert_eq!(got, want, "столбцы или их типы не те, что в условии:\n{t}");
    }

    #[tokio::test]
    async fn rows_can_be_inserted() {
        let db = TestDb::new().await;
        db.run(users_table()).await;
        db.run("INSERT INTO users VALUES (3000000000, 'alice', 'alice@example.com', now())").await;
    }
}
