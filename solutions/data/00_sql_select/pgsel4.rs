fn alice_id() -> &'static str {
    "SELECT id, username FROM users WHERE username = 'alice'"
}

fn german_users() -> &'static str {
    "SELECT username FROM users WHERE country = 'DE'"
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

    #[tokio::test]
    async fn finds_alice() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(alice_id()).await;
        assert_eq!(t.len(), 1, "должна найтись ровно одна alice:\n{t}");
        assert_eq!(t.cell(0, 0), "1", "у alice id = 1");
    }

    #[tokio::test]
    async fn finds_germans() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(german_users()).await;
        assert_eq!(set(t.col("username")), set(vec!["alice", "carol", "Karl"]), "из Германии (DE) три пользователя:\n{t}");
    }
}
