fn active_de_or_fr() -> &'static str {
    "SELECT username FROM users WHERE status = 'active' AND (country = 'DE' OR country = 'FR')"
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
    async fn only_active_from_de_or_fr() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(active_de_or_fr()).await;
        assert_eq!(
            set(t.col("username")),
            set(vec!["alice", "dave", "grace", "Karl"]),
            "AND выполняется раньше OR: в ответ попали не только активные. Расставь скобки:\n{t}"
        );
    }
}
