// Кнопка «загрузить ещё» в чате: пользователь долистал до сообщения, отправленного в момент 400, и хочет
// увидеть два сообщения, которые были перед ним. Таблица уже хранит сообщения свежими вперёд. Само
// сообщение 400 на экране уже есть, его повторять нельзя.
// TODO: Верни sent_at и body двух сообщений чата 1, отправленных раньше 400, свежие первыми.

fn page_before() -> &'static str {
    "SELECT sent_at, body FROM messages WHERE chat_id = 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)";
    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'сообщение 100'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'ann', 'сообщение 200'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 300, 'ann', 'сообщение 300'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 400, 'ann', 'сообщение 400'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 500, 'ann', 'сообщение 500'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 600, 'ann', 'сообщение 600'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 350, 'ann', 'сообщение 350')";

    #[tokio::test]
    async fn two_messages_before_400() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let t = ks.table(page_before()).await;
        assert_eq!(t.column(0), ["300", "200"], "нужны два сообщения перед 400, без самого 400 и без чужого чата\n{t}");
        assert_eq!(t.col("body"), ["сообщение 300", "сообщение 200"], "второй столбец это body\n{t}");
    }
}
