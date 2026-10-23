// IS DISTINCT FROM это «не равно», которое умеет работать с NULL: NULL считается обычным
// значением, поэтому `NULL IS DISTINCT FROM 'x'` это true, а `NULL IS DISTINCT FROM NULL`
// это false. Парная форма: IS NOT DISTINCT FROM.

fn custom_names() -> &'static str {
    // TODO: ники тех, у кого display_name не совпадает с username
    // (если display_name пусто, это тоже «не совпадает»)
    "SELECT username FROM users"
}

fn not_pending() -> &'static str {
    // TODO: ники всех, у кого статус не 'pending', включая тех, у кого статуса нет
    "SELECT username FROM users"
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
