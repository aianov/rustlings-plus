// Модель в Scylla начинается не с сущностей, а со списка запросов. Экраны чата делают три запроса:
// Q1 последние сообщения чата (свежие первыми), Q2 чаты пользователя, Q3 пользователь по нику.
// Каждый запрос должен читать одну партицию и не просить ALLOW FILTERING. Таблицы и столбцы такие:
// messages_by_chat (chat_id int, sent_at bigint, author text, body text),
// chats_by_user (user_id int, chat_id int, title text), users_by_name (username text, user_id int).
// TODO: Напиши три CREATE TABLE через точку с запятой с ключами под эти запросы.

fn schema() -> &'static str {
    ""
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'раз');
        INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'два');
        INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES (1, 300, 'ann', 'три');
        INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES (2, 400, 'kate', 'чужое');
        INSERT INTO chats_by_user (user_id, chat_id, title) VALUES (7, 1, 'Rust');
        INSERT INTO chats_by_user (user_id, chat_id, title) VALUES (7, 3, 'Go');
        INSERT INTO chats_by_user (user_id, chat_id, title) VALUES (9, 1, 'Rust');
        INSERT INTO users_by_name (username, user_id) VALUES ('kate', 7);
        INSERT INTO users_by_name (username, user_id) VALUES ('bob', 9)";

    async fn ready() -> TestKs {
        assert!(!schema().trim().is_empty(), "сначала напиши три таблицы");
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        ks
    }

    #[tokio::test]
    async fn q1_latest_messages() {
        let ks = ready().await;
        let t = ks.table("SELECT author, body FROM messages_by_chat WHERE chat_id = 1 LIMIT 2").await;
        assert_eq!(t.rows, [["ann", "три"], ["bob", "два"]], "Q1: два последних сообщения чата, свежие первыми\n{t}");
    }

    #[tokio::test]
    async fn q2_chats_of_user() {
        let ks = ready().await;
        let t = ks.table("SELECT chat_id, title FROM chats_by_user WHERE user_id = 7").await.sorted();
        assert_eq!(t.rows, [["1", "Rust"], ["3", "Go"]], "Q2: оба чата пользователя 7\n{t}");
    }

    #[tokio::test]
    async fn q3_user_by_name() {
        let ks = ready().await;
        let t = ks.table("SELECT user_id FROM users_by_name WHERE username = 'kate'").await;
        assert_eq!(t.column(0), ["7"], "Q3: kate это пользователь 7\n{t}");
    }
}
