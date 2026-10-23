// Профилю автора нужен запрос «сообщения автора, свежие первыми», а писать каждое сообщение руками в две
// таблицы команда не хочет. Scylla умеет вести такую копию сама: материализованное представление строится из
// базовой таблицы с другим ключом и обновляется при каждой записи в неё. Базовая таблица messages (chat_id,
// sent_at, author, body), ключ (chat_id, sent_at).
// TODO: Создай представление `messages_by_author` с ключом партиции author, свежие первыми, без потерь
// одновременных сообщений из разных чатов.

fn view() -> &'static str {
    "CREATE INDEX ON messages (author)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use std::time::Duration;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    async fn eventually(ks: &TestKs, cql: &str, rows: usize) -> course_support::scylla::Table {
        for _ in 0..100 {
            if let Ok(t) = ks.try_table(cql).await {
                if t.len() == rows {
                    return t;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ks.table(cql).await
    }

    #[tokio::test]
    async fn view_follows_the_base_table() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(view()).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'bob', 'раз');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 200, 'bob', 'два');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (3, 200, 'bob', 'одновременно');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 150, 'ann', 'не bob')").await;
        let q = "SELECT chat_id, sent_at, body FROM messages_by_author WHERE author = 'bob'";
        let t = eventually(&ks, q, 3).await;
        assert_eq!(t.rows, [["2", "200", "два"], ["3", "200", "одновременно"], ["1", "100", "раз"]], "сообщения bob из всех чатов, свежие первыми\n{t}");
        ks.run("UPDATE messages SET body = 'раз (правка)' WHERE chat_id = 1 AND sent_at = 100").await;
        let mut seen = String::new();
        for _ in 0..100 {
            seen = ks.table("SELECT body FROM messages_by_author WHERE author = 'bob' AND sent_at = 100 AND chat_id = 1").await.column(0).join("");
            if seen == "раз (правка)" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(seen, "раз (правка)", "правка в базовой таблице доходит до представления");
    }
}
