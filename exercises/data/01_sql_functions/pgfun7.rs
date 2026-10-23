// Поиск по профилям: нужны все пользователи, кроме тех, у кого био равно пустой строке (такие
// есть, их оставили после чистки). Тех, у кого био не заполнено вовсе (NULL), показывать нужно.
// Запрос так и написан, но в ответе недосчитались и тех, кто био не заполнял.
// TODO: Верни пользователей без био, не ломая остальное.

fn not_empty_bio() -> &'static str {
    "SELECT username FROM users WHERE bio <> ''"
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
