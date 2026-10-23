fn not_empty_bio() -> &'static str {
    "SELECT username FROM users WHERE bio <> '' OR bio IS NULL"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn null_bio_is_kept() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(not_empty_bio()).await;
        let mut got = t.col("username");
        got.sort();
        assert_eq!(
            got,
            ["alice", "bob", "dave", "eve", "frank", "grace", "heidi", "kenji"],
            "bob, eve и heidi не заполняли био (NULL), а значит это не пустая строка, их нужно вернуть; carol с пустой строкой лишняя, dave с пробелами остаётся. Условие с NULL молча теряет строки:\n{t}"
        );
    }
}
