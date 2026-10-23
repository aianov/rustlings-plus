fn schema() -> &'static str {
    "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'привет'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'привет, ann'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 300, 'ann', 'как дела?'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 150, 'kate', 'кто здесь?')";

    #[tokio::test]
    async fn chat_keeps_every_message() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        let t = ks.table("SELECT sent_at, author FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["100", "ann"], ["200", "bob"], ["300", "ann"]], "в чате 1 три сообщения по порядку отправки\n{t}");
        let other = ks.table("SELECT sent_at FROM messages WHERE chat_id = 2").await;
        assert_eq!(other.column(0), ["150"], "у чата 2 своё сообщение\n{other}");
    }
}
