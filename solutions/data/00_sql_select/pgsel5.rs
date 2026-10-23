fn older_than_30() -> &'static str {
    "SELECT username FROM users WHERE age > 30"
}

fn active_germans() -> &'static str {
    "SELECT username FROM users WHERE country = 'DE' AND status = 'active'"
}

fn french_or_british() -> &'static str {
    "SELECT username FROM users WHERE country = 'FR' OR country = 'GB'"
}

fn not_active() -> &'static str {
    "SELECT username FROM users WHERE NOT status = 'active'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn set(mut v: Vec<&str>) -> Vec<&str> {
        v.sort();
        v
    }

    async fn names(sql: &str) -> Vec<String> {
        let db = TestDb::with_sql(SEED).await;
        db.table(sql).await.col("username").into_iter().map(String::from).collect()
    }

    fn check(got: Vec<String>, want: Vec<&str>, what: &str) {
        let got = set(got.iter().map(String::as_str).collect());
        assert_eq!(got, set(want), "{what}: ответ не совпал с ожидаемым");
    }

    #[tokio::test]
    async fn comparison() {
        check(names(older_than_30()).await, vec!["bob", "carol", "frank", "grace", "ivan", "Karl"], "age > 30");
    }

    #[tokio::test]
    async fn and_condition() {
        check(names(active_germans()).await, vec!["alice", "Karl"], "DE и active");
    }

    #[tokio::test]
    async fn or_condition() {
        check(names(french_or_british()).await, vec!["dave", "frank", "grace", "heidi"], "FR или GB");
    }

    #[tokio::test]
    async fn not_condition() {
        check(names(not_active()).await, vec!["carol", "eve", "heidi", "judy"], "NOT active");
    }
}
