fn age_25_to_35() -> &'static str {
    "SELECT username FROM users WHERE age BETWEEN 25 AND 35"
}

fn selected_countries() -> &'static str {
    "SELECT username FROM users WHERE country IN ('DE', 'FR', 'GB')"
}

fn other_countries() -> &'static str {
    "SELECT username FROM users WHERE country NOT IN ('DE', 'FR', 'GB')"
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
    async fn between_includes_both_ends() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(age_25_to_35()).await;
        assert_eq!(
            set(t.col("username")),
            set(vec!["alice", "bob", "eve", "grace", "judy"]),
            "BETWEEN включает обе границы: eve ровно 25 лет, ей место в ответе:\n{t}"
        );
    }

    #[tokio::test]
    async fn in_list() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(selected_countries()).await;
        assert_eq!(
            set(t.col("username")),
            set(vec!["alice", "carol", "dave", "frank", "grace", "heidi", "Karl"]),
            "ждём пользователей из DE, FR и GB:\n{t}"
        );
    }

    #[tokio::test]
    async fn not_in_list() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(other_countries()).await;
        assert_eq!(
            set(t.col("username")),
            set(vec!["bob", "bob_2", "eve", "ivan", "judy"]),
            "ждём всех, кто не из DE, FR и GB:\n{t}"
        );
    }
}
