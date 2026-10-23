// Модератор разбирает жалобы. Нужно три удаления одним скриптом (команды через точку с запятой):
// 1) спам bob в чате 1 (sent_at 200) удалить целиком;
// 2) в сообщении ann в чате 1 (sent_at 300) убрать только текст: автор и время остаются, текст становится null;
// 3) в чате 2 удалить всё, что отправлено раньше 200.
// TODO: Верни скрипт из трёх команд DELETE.

fn moderate() -> &'static str {
    ""
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";
    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'привет'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'купи крипту'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 300, 'ann', 'мой пароль 1234'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 400, 'bob', 'ок'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 50, 'kate', 'старое'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 150, 'kate', 'тоже старое'); INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 250, 'kate', 'новое')";

    #[tokio::test]
    async fn three_kinds_of_delete() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        ks.run(moderate()).await;
        let one = ks.table("SELECT sent_at, author, body FROM messages WHERE chat_id = 1").await;
        assert_eq!(
            one.rows,
            [["100", "ann", "привет"], ["300", "ann", "NULL"], ["400", "bob", "ок"]],
            "в чате 1 спам удалён целиком, у сообщения 300 остался автор без текста\n{one}"
        );
        let two = ks.table("SELECT sent_at FROM messages WHERE chat_id = 2").await;
        assert_eq!(two.column(0), ["250"], "в чате 2 осталось только то, что отправлено не раньше 200\n{two}");
    }
}
