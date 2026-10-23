fn schema() -> &'static str {
    "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'сообщение 100'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'ann', 'сообщение 200'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 300, 'ann', 'сообщение 300'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 400, 'ann', 'сообщение 400'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 500, 'ann', 'сообщение 500')";

    #[tokio::test]
    async fn latest_three_come_first() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        let t = ks.table("SELECT sent_at FROM messages WHERE chat_id = 1 LIMIT 3").await;
        assert_eq!(t.column(0), ["500", "400", "300"], "LIMIT 3 должен отдать три самых свежих сообщения\n{t}");
    }
}
