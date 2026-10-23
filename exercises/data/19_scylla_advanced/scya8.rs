// Сообщение пишется в две таблицы: в чат и в профиль автора. Если сервис упадёт между двумя записями,
// сообщение окажется только в одной, и чат с профилем разойдутся навсегда. Скрипт ниже выполняется одной
// командой (сессия не делит его на части), и Scylla его отвергает. Нужна запись, которая гарантированно
// дойдёт до обеих таблиц: либо обе, либо, если клиент не дождался ответа, обе позже.

fn send(chat: i32, author: &str, sent_at: i64, body: &str) -> String {
    format!(
        "INSERT INTO messages_by_chat (chat_id, sent_at, author, body) VALUES ({chat}, {sent_at}, '{author}', '{body}');
         INSERT INTO messages_by_author (author, sent_at, chat_id, body) VALUES ('{author}', {sent_at}, {chat}, '{body}')"
    )
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages_by_chat (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at));
        CREATE TABLE messages_by_author (author text, sent_at bigint, chat_id int, body text, PRIMARY KEY (author, sent_at, chat_id))";

    #[tokio::test]
    async fn both_tables_in_one_logged_batch() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let text = send(1, "bob", 100, "привет");
        assert!(!text.to_uppercase().contains("UNLOGGED"), "для двух таблиц нужна гарантия доставки: logged batch");
        ks.session.query(text, ()).await.expect("одна команда пишет обе таблицы");
        let chat = ks.table("SELECT body FROM messages_by_chat WHERE chat_id = 1").await;
        let author = ks.table("SELECT body FROM messages_by_author WHERE author = 'bob'").await;
        assert_eq!(chat.column(0), ["привет"], "сообщение в чате\n{chat}");
        assert_eq!(author.column(0), ["привет"], "и в профиле автора\n{author}");
    }
}
