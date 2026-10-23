fn schema() -> &'static str {
    "CREATE TABLE messages (chat_id int, sent_at timeuuid, author text, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, f1016000-cd46-11f1-8001-0000000000a1, 'ann', 'одновременно 1'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, f1016000-cd46-11f1-8002-0000000000b2, 'bob', 'одновременно 2'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, f1022350-cd46-11f1-8003-0000000000a1, 'ann', 'чуть позже')";

    #[tokio::test]
    async fn simultaneous_messages_survive() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        let t = ks.table("SELECT author, body, toTimestamp(sent_at) FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.len(), 3, "все три сообщения на месте, одновременные не затёрли друг друга\n{t}");
        assert_eq!(t.cell(0, 1), "чуть позже", "свежее сообщение первым\n{t}");
        assert_eq!(t.column(2)[1], "1792584000000", "из ключа по-прежнему достаётся время отправки\n{t}");
    }
}
