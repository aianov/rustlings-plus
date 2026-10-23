fn send_ephemeral(chat: i32, sent_at: i64, body: &str, ttl_secs: u32) -> String {
    let body = body.replace('\'', "''");
    format!("INSERT INTO messages (chat_id, sent_at, body) VALUES ({chat}, {sent_at}, '{body}') USING TTL {ttl_secs}")
}

fn stories_schema() -> &'static str {
    "CREATE TABLE stories (user_id int, posted_at bigint, body text, PRIMARY KEY (user_id, posted_at)) WITH default_time_to_live = 86400"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use std::time::Duration;

    #[tokio::test]
    async fn message_disappears_by_itself() {
        let ks = TestKs::with_cql("CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at))").await;
        ks.run(&send_ephemeral(1, 100, "исчезну", 2)).await;
        ks.run(&send_ephemeral(1, 200, "останусь", 3600)).await;
        let t = ks.table("SELECT body, TTL(body) FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.len(), 1, "сразу после отправки сообщение видно\n{t}");
        let ttl: i64 = t.cell(0, 1).parse().unwrap_or(0);
        assert!((1..=2).contains(&ttl), "у сообщения осталось не больше двух секунд жизни, а TTL = {ttl}");
        tokio::time::sleep(Duration::from_millis(3500)).await;
        let t = ks.table("SELECT body FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.column(0), ["останусь"], "через три с половиной секунды исчезло только короткоживущее\n{t}");
    }

    #[tokio::test]
    async fn stories_live_a_day_by_default() {
        let ks = TestKs::with_cql(stories_schema()).await;
        let t = ks
            .table(&format!("SELECT default_time_to_live FROM system_schema.tables WHERE keyspace_name = '{}' AND table_name = 'stories'", ks.name()))
            .await;
        assert_eq!(t.column(0), ["86400"], "срок жизни по умолчанию у таблицы сутки\n{t}");
        ks.run("INSERT INTO stories (user_id, posted_at, body) VALUES (7, 1, 'утро')").await;
        let ttl = ks.table("SELECT TTL(body) FROM stories WHERE user_id = 7").await;
        let left: i64 = ttl.cell(0, 0).parse().unwrap_or(0);
        assert!(left > 86000, "обычный INSERT получает срок таблицы: {left}");
    }
}
