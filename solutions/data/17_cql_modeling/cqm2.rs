fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn send(chat: i32, author: &str, sent_at: i64, body: &str) -> String {
    let (author, body) = (quote(author), quote(body));
    format!(
        "INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES ({chat}, {sent_at}, {author}, {body});
         INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ({author}, {sent_at}, {chat}, {body})"
    )
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages_by_chat (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC);
        CREATE TABLE messages_by_author (author text, sent_at bigint, chat_id int, body text, PRIMARY KEY (author, sent_at, chat_id)) WITH CLUSTERING ORDER BY (sent_at DESC, chat_id ASC)";

    #[tokio::test]
    async fn message_lands_in_both_tables() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(&send(1, "bob", 100, "привет")).await;
        ks.run(&send(2, "bob", 200, "второй чат")).await;
        ks.run(&send(1, "ann", 300, "ответ")).await;
        let chat = ks.table("SELECT author, body FROM messages_by_chat WHERE chat_id = 1").await;
        assert_eq!(chat.rows, [["ann", "ответ"], ["bob", "привет"]], "чат 1 видит свои сообщения\n{chat}");
        let bob = ks.table("SELECT chat_id, body FROM messages_by_author WHERE author = 'bob'").await;
        assert_eq!(bob.rows, [["2", "второй чат"], ["1", "привет"]], "профиль bob видит его сообщения из всех чатов\n{bob}");
    }

    #[tokio::test]
    async fn any_text_is_stored_as_is() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let tricky = "it's fine; DROP TABLE messages_by_chat; '' ok";
        ks.run(&send(1, "o'neil", 100, tricky)).await;
        let chat = ks.table("SELECT author, body FROM messages_by_chat WHERE chat_id = 1").await;
        assert_eq!(chat.rows, [["o'neil", tricky]], "текст и имя сохраняются как есть\n{chat}");
        let by = ks.table("SELECT body FROM messages_by_author WHERE author = 'o''neil'").await;
        assert_eq!(by.column(0), [tricky], "и во второй таблице тоже\n{by}");
    }
}
