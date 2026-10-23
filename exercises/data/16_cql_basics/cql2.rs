// Профили пользователей ищутся только по id: открыть профиль, показать автора сообщения. Значит,
// user_id это ключ таблицы. В CQL у каждой таблицы обязан быть первичный ключ, без него таблицу не создать.
// TODO: Создай таблицу `profiles` (user_id int, username text, bio text) с ключом user_id и напиши запрос,
// который достаёт username и bio пользователя 7.

fn schema() -> &'static str {
    "CREATE TABLE profiles (user_id int, username text, bio text)"
}

fn lookup() -> &'static str {
    "SELECT * FROM profiles"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO profiles (user_id, username, bio) VALUES (3, 'ann', 'люблю SQL'); INSERT INTO profiles (user_id, username, bio) VALUES (7, 'kate', 'пишу на Rust'); INSERT INTO profiles (user_id, username, bio) VALUES (9, 'bob', 'просто bob')";

    #[tokio::test]
    async fn profile_by_id() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        let t = ks.table(lookup()).await;
        assert_eq!(t.columns, ["username", "bio"], "нужны два столбца: username и bio\n{t}");
        assert_eq!(t.rows, [["kate", "пишу на Rust"]], "нужен ровно пользователь 7\n{t}");
    }
}
