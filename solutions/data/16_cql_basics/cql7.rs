fn schema() -> &'static str {
    "CREATE TABLE messages (chat_id int, month int, sent_at bigint, body text, PRIMARY KEY ((chat_id, month), sent_at))"
}

fn october() -> &'static str {
    "SELECT sent_at, body FROM messages WHERE chat_id = 1 AND month = 202610"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202609, 10, 'сентябрь'); INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202610, 20, 'октябрь 1'); INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202610, 30, 'октябрь 2'); INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202611, 40, 'ноябрь'); INSERT INTO messages (chat_id, month, sent_at, body) VALUES (2, 202610, 25, 'чужой чат')";

    #[tokio::test]
    async fn chat_and_month_form_the_partition() {
        let ks = TestKs::with_cql(schema()).await;
        let t = ks
            .table(&format!("SELECT column_name, kind FROM system_schema.columns WHERE keyspace_name = '{}' AND table_name = 'messages'", ks.name()))
            .await
            .sorted();
        let kinds: Vec<(&str, &str)> = t.rows.iter().map(|r| (r[0].as_str(), r[1].as_str())).collect();
        assert!(kinds.contains(&("chat_id", "partition_key")) && kinds.contains(&("month", "partition_key")), "chat_id и month оба должны входить в ключ партиции\n{t}");
        assert!(kinds.contains(&("sent_at", "clustering")), "sent_at остаётся ключом кластеризации\n{t}");
    }

    #[tokio::test]
    async fn october_of_chat_one() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        let t = ks.table(october()).await;
        assert_eq!(t.col("body"), ["октябрь 1", "октябрь 2"], "только октябрь чата 1\n{t}");
    }
}
