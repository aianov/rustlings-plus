// Правки сообщений идут через очередь, и иногда старая правка доходит позже новой: пользователь исправил
// опечатку, потом переписал сообщение, а через секунду с задержкой пришла первая правка и вернула опечатку.
// У каждой правки есть момент, когда её сделал пользователь (микросекунды). Сделай так, чтобы побеждала
// правка с более поздним моментом, а не та, что дошла последней.

fn edit(chat: i32, sent_at: i64, body: &str, edited_at_us: i64) -> String {
    let _ = edited_at_us;
    format!("UPDATE messages SET body = '{body}' WHERE chat_id = {chat} AND sent_at = {sent_at}")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn later_edit_wins_regardless_of_arrival() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let now_us = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
        let base = i64::try_from(now_us).unwrap_or(i64::MAX) - 3_600_000_000;
        ks.run(&format!("INSERT INTO messages (chat_id, sent_at, body) VALUES (1, 100, 'прифет') USING TIMESTAMP {base}")).await;
        ks.run(&edit(1, 100, "переписал целиком", base + 2_000)).await;
        ks.run(&edit(1, 100, "привет", base + 1_000)).await;
        let t = ks.table("SELECT body, WRITETIME(body) FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.cell(0, 0), "переписал целиком", "опоздавшая старая правка не перетирает новую\n{t}");
        assert_eq!(t.cell(0, 1), (base + 2_000).to_string(), "время записи это момент правки\n{t}");
    }
}
