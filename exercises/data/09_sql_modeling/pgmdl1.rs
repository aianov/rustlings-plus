// Схема постов хранит имя автора прямо в каждом посте (author_name): так ленту читать без JOIN.
// Но пользователь меняет ник, и старые посты продолжают показывать прежний: имя расходится
// с настоящим. Ник должен храниться ровно в одном месте. Нужна схема (`schema`) и запрос ленты
// (`feed`): столбцы id, username, body, порядок по id поста, ник всегда актуальный. Посты вставляются
// только с author_id и body.
// TODO: Найди причину расхождения и перепиши схему и запрос.

fn schema() -> &'static str {
    "CREATE TABLE users (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         username text NOT NULL UNIQUE
     );
     CREATE TABLE posts (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         author_id bigint NOT NULL REFERENCES users (id),
         author_name text NOT NULL,
         body text NOT NULL
     )"
}

fn feed() -> &'static str {
    "SELECT id, author_name AS username, body FROM posts ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn renamed_user_is_renamed_everywhere() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice'); INSERT INTO posts (author_id, body) VALUES (1, 'one'), (1, 'two')").await;
        let t = db.table(feed()).await;
        assert_eq!(t.columns, ["id", "username", "body"], "нужны столбцы id, username, body");
        db.run("UPDATE users SET username = 'alice_new' WHERE id = 1").await;
        let t = db.table(feed()).await;
        assert_eq!(
            t.rows,
            [["1", "alice_new", "one"], ["2", "alice_new", "two"]],
            "после смены ника все посты должны показывать новый ник, а лента показывает старый:\n{t}"
        );
    }

    #[tokio::test]
    async fn name_is_stored_once() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        let t = db.table("SELECT column_name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name LIKE '%name%'").await;
        assert!(t.is_empty(), "в posts не должно остаться столбца с копией имени:\n{t}");
    }
}
