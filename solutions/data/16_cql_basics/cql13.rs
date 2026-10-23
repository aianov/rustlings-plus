fn last_per_chat() -> &'static str {
    "SELECT chat_id, sent_at, body FROM messages WHERE chat_id IN (1, 2, 3) PER PARTITION LIMIT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)";
    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, body) VALUES (1, 100, 'старое в 1'); INSERT INTO messages (chat_id, sent_at, body) VALUES (1, 300, 'свежее в 1'); INSERT INTO messages (chat_id, sent_at, body) VALUES (2, 200, 'одно в 2'); INSERT INTO messages (chat_id, sent_at, body) VALUES (3, 50, 'старое в 3'); INSERT INTO messages (chat_id, sent_at, body) VALUES (3, 150, 'среднее в 3'); INSERT INTO messages (chat_id, sent_at, body) VALUES (3, 250, 'свежее в 3'); INSERT INTO messages (chat_id, sent_at, body) VALUES (4, 999, 'чат 4 не просили')";

    #[tokio::test]
    async fn one_latest_message_per_chat() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let t = ks.table(last_per_chat()).await.sorted();
        assert_eq!(
            t.rows,
            [["1", "300", "свежее в 1"], ["2", "200", "одно в 2"], ["3", "250", "свежее в 3"]],
            "по одной самой свежей строке в каждом из трёх чатов\n{t}"
        );
    }
}
