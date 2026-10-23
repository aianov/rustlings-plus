// Профиль пользователя показывает его последние сообщения во всех чатах. Запрос по столбцу author
// таблица messages не умеет: Scylla требует ALLOW FILTERING. С ним запрос заработает, но будет читать все
// сообщения всех чатов кластера ради нескольких строк. В Scylla под новый запрос заводят новую таблицу.
// Создай таблицу `messages_by_author` (author, sent_at, chat_id, body) под этот запрос и читай из неё
// сообщения bob: chat_id, sent_at, body, свежие первыми. Приложение пишет каждое сообщение в обе таблицы,
// тест делает так же. Учти: в разные чаты можно написать в одну и ту же миллисекунду.

fn schema() -> &'static str {
    "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))"
}

fn by_author() -> &'static str {
    "SELECT chat_id, sent_at, body FROM messages WHERE author = 'bob'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'привет'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('ann', 100, 1, 'привет'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'кто здесь'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('bob', 200, 1, 'кто здесь'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 200, 'bob', 'и тут я'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('bob', 200, 2, 'и тут я'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 300, 'bob', 'пока'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('bob', 300, 1, 'пока'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (3, 150, 'kate', 'hi'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('kate', 150, 3, 'hi'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 400, 'bob', 'вернулся'); INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('bob', 400, 2, 'вернулся')";

    #[tokio::test]
    async fn bob_messages_newest_first() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        assert!(!by_author().to_uppercase().contains("ALLOW FILTERING"), "ALLOW FILTERING читает весь кластер, нужна отдельная таблица");
        assert!(by_author().contains("messages_by_author"), "запрос должен читать таблицу messages_by_author");
        let t = ks.table(by_author()).await;
        let rows: Vec<(&str, &str)> = t.rows.iter().map(|r| (r[0].as_str(), r[1].as_str())).collect();
        assert_eq!(rows.len(), 4, "у bob четыре сообщения, два из них в одну миллисекунду в разных чатах\n{t}");
        assert_eq!(t.col("sent_at"), ["400", "300", "200", "200"], "свежие первыми\n{t}");
        assert!(rows.contains(&("1", "200")) && rows.contains(&("2", "200")), "сообщения в одну миллисекунду в разные чаты не затирают друг друга\n{t}");
    }
}
