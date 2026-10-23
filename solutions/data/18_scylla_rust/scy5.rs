use scylla::{FromRow, Session};
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug, PartialEq, FromRow)]
struct Message {
    author: String,
    body: String,
    sent_at: i64,
}

async fn history(session: &Session, chat: i32) -> Result<Vec<Message>, BoxError> {
    let res = session.query("SELECT author, body, sent_at FROM messages WHERE chat_id = ?", (chat,)).await?;
    Ok(res.rows_typed::<Message>()?.collect::<Result<_, _>>()?)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    fn expected() -> Vec<Message> {
        vec![
            Message { author: "ann".to_string(), body: "раз".to_string(), sent_at: 100 },
            Message { author: "bob".to_string(), body: "два".to_string(), sent_at: 200 },
        ]
    }

    async fn ready() -> TestKs {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'раз');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'два')").await;
        ks
    }

    #[tokio::test]
    async fn fields_get_their_columns() {
        let ks = ready().await;
        assert_eq!(history(&ks.session, 1).await.expect("история"), expected());
    }

    #[tokio::test]
    async fn new_column_does_not_break_reading() {
        let ks = ready().await;
        ks.run("ALTER TABLE messages ADD pinned boolean").await;
        assert_eq!(history(&ks.session, 1).await.expect("история после ALTER TABLE"), expected(), "новый столбец в таблице не ломает чтение");
    }
}
