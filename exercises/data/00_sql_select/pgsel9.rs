// Хотим найти тех, кто ни разу не заходил (last_login_at пуст), и наоборот, тех, кто заходил.
// Оба запроса написаны «как в Rust» и оба возвращают пустоту. NULL в SQL означает «значение
// неизвестно», а сравнить неизвестное с чем угодно даёт не «нет», а снова «неизвестно».
// TODO: Почини оба запроса.

fn never_logged_in() -> &'static str {
    "SELECT username FROM users WHERE last_login_at = NULL"
}

fn logged_in() -> &'static str {
    "SELECT username FROM users WHERE last_login_at <> NULL"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn never_logged_in_found() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(never_logged_in()).await;
        let mut got = t.col("username");
        got.sort();
        assert_eq!(got, ["dave", "eve", "heidi"], "пустой last_login_at у dave, eve и heidi, а нашлось:\n{t}");
    }

    #[tokio::test]
    async fn logged_in_found() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(logged_in()).await;
        assert_eq!(t.len(), 9, "заходили девять пользователей из двенадцати:\n{t}");
    }
}
