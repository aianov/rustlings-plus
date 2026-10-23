// История теперь читается в структуру Message через derive(FromRow). Всё работало, пока модератор не скрыл
// текст одного сообщения (`DELETE body ...`): теперь история всего чата падает с ошибкой разбора, и чат не
// открывается ни у кого. Почини так, чтобы сообщение без текста читалось, а не роняло историю.

use scylla::{FromRow, Session};
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug, PartialEq, FromRow)]
struct Message {
    sent_at: i64,
    author: String,
    body: String,
}

async fn history(session: &Session, chat: i32) -> Result<Vec<Message>, BoxError> {
    let res = session.query("SELECT sent_at, author, body FROM messages WHERE chat_id = ?", (chat,)).await?;
    Ok(res.rows_typed::<Message>()?.collect::<Result<_, _>>()?)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn hidden_text_reads_as_none() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'привет');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'спам');
                DELETE body FROM messages WHERE chat_id = 1 AND sent_at = 200").await;
        let got = history(&ks.session, 1).await.expect("история со скрытым текстом должна читаться");
        assert_eq!(
            got,
            [
                Message { sent_at: 100, author: "ann".to_string(), body: Some("привет".to_string()) },
                Message { sent_at: 200, author: "bob".to_string(), body: None },
            ],
            "скрытый текст это None, остальное как было"
        );
    }
}
