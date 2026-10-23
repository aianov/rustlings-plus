fn hour() -> &'static str {
    "SELECT body, toTimestamp(id) AS sent_at FROM messages WHERE chat_id = 1 AND id >= minTimeuuid('2026-10-20 10:00:00+0000') AND id < minTimeuuid('2026-10-20 11:00:00+0000')"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, id timeuuid, body text, PRIMARY KEY (chat_id, id))";
    const ROWS: &str = "INSERT INTO messages (chat_id, id, body) VALUES (1, 030ea8f0-cc6d-11f1-8001-1a2b3c4d5e6f, 'до начала'); INSERT INTO messages (chat_id, id, body) VALUES (1, 030ed000-cc6d-11f1-8002-1a2b3c4d5e6f, 'ровно в 10'); INSERT INTO messages (chat_id, id, body) VALUES (1, 33f10400-cc71-11f1-8003-1a2b3c4d5e6f, 'в 10:30'); INSERT INTO messages (chat_id, id, body) VALUES (1, 64d310f0-cc75-11f1-8004-1a2b3c4d5e6f, 'за миг до 11'); INSERT INTO messages (chat_id, id, body) VALUES (1, 64d33800-cc75-11f1-8005-1a2b3c4d5e6f, 'ровно в 11'); INSERT INTO messages (chat_id, id, body) VALUES (1, 95b56c00-cc79-11f1-8006-1a2b3c4d5e6f, 'в 11:30'); INSERT INTO messages (chat_id, id, body) VALUES (2, 1b7fea00-cc6f-11f1-8007-1a2b3c4d5e6f, 'чужой чат')";

    #[tokio::test]
    async fn messages_of_one_hour() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let t = ks.table(hour()).await;
        assert_eq!(t.col("body"), ["ровно в 10", "в 10:30", "за миг до 11"], "начало часа включается, конец нет, чужой чат не нужен\n{t}");
        assert_eq!(t.col("sent_at"), ["1792490400000", "1792492200000", "1792493999999"], "sent_at это время из id в миллисекундах\n{t}");
    }
}
