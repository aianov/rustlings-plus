fn custom_names() -> &'static str {
    "SELECT username FROM users WHERE display_name IS DISTINCT FROM username"
}

fn not_pending() -> &'static str {
    "SELECT username FROM users WHERE status IS DISTINCT FROM 'pending'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn custom_display_names() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(custom_names()).await;
        let mut got = t.col("username");
        got.sort();
        assert_eq!(
            got,
            ["alice", "bob", "carol", "dave", "eve", "frank", "heidi", "kenji"],
            "не совпадает у всех, кроме grace (у неё display_name равен нику). NULL тоже считается отличием:\n{t}"
        );
    }

    #[tokio::test]
    async fn status_with_null() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(not_pending()).await;
        assert_eq!(t.len(), 8, "не pending все, кроме frank, включая dave и eve без статуса:\n{t}");
    }
}
