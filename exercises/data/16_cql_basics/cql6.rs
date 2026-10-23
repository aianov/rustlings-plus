// Сообщения лежат по чату, внутри по дню и по времени от начала дня в секундах. Нужен запрос «сообщения
// чата 1 за 21 октября 2026 после полудня (строго позже 43200 секунд)», а Scylla отвергает его с ошибкой про
// предыдущий столбец ключа. Схему менять нельзя, ALLOW FILTERING тоже: разберись, какие условия CQL
// разрешает в WHERE, и перепиши запрос. Столбцы ответа: sent_at и body.

fn afternoon() -> &'static str {
    "SELECT sent_at, body FROM messages WHERE chat_id = 1 AND sent_at > 43200"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, day int, sent_at int, body text, PRIMARY KEY (chat_id, day, sent_at))";
    const ROWS: &str = "INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261020, 50000, 'вчера вечером'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261021, 30000, 'утро'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261021, 43200, 'ровно полдень'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261021, 50000, 'после обеда'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261021, 70000, 'вечер'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (1, 20261022, 45000, 'завтра'); INSERT INTO messages (chat_id, day, sent_at, body) VALUES (2, 20261021, 60000, 'чужой чат')";

    #[tokio::test]
    async fn afternoon_of_the_21st() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        assert!(!afternoon().to_uppercase().contains("ALLOW FILTERING"), "ALLOW FILTERING читает лишнее, обойдись без него");
        let t = ks.table(afternoon()).await;
        assert_eq!(t.col("body"), ["после обеда", "вечер"], "только 21 октября, строго после полудня, по порядку времени\n{t}");
    }
}
