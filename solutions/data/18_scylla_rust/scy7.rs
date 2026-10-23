use scylla::prepared_statement::PreparedStatement;
use scylla::transport::errors::QueryError;
use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

struct Store {
    insert: PreparedStatement,
    history: PreparedStatement,
}

impl Store {
    async fn new(session: &Session) -> Result<Store, QueryError> {
        let insert = session.prepare("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (?, ?, ?, ?)").await?;
        let history = session.prepare("SELECT sent_at, body FROM messages WHERE chat_id = ?").await?;
        Ok(Store { insert, history })
    }

    async fn send(&self, session: &Session, chat: i32, sent_at: i64, author: &str, body: &str) -> Result<(), QueryError> {
        session.execute(&self.insert, (chat, sent_at, author, body)).await?;
        Ok(())
    }

    async fn history(&self, session: &Session, chat: i32) -> Result<Vec<(i64, String)>, BoxError> {
        let res = session.execute(&self.history, (chat,)).await?;
        Ok(res.rows_typed::<(i64, String)>()?.collect::<Result<_, _>>()?)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn prepared_once_executed_many() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let store = Store::new(&ks.session).await.expect("подготовка запросов");
        assert!(store.insert.get_statement().trim_start().to_uppercase().starts_with("INSERT"), "insert это подготовленная вставка");
        assert!(store.history.get_statement().trim_start().to_uppercase().starts_with("SELECT"), "history это подготовленное чтение");
        for (t, body) in [(300, "три"), (100, "раз"), (200, "два")] {
            store.send(&ks.session, 1, t, "ann", body).await.expect("отправка");
        }
        store.send(&ks.session, 2, 50, "bob", "чужое").await.expect("отправка");
        let got = store.history(&ks.session, 1).await.expect("история");
        assert_eq!(got, [(100, "раз".to_string()), (200, "два".to_string()), (300, "три".to_string())], "история чата 1 по порядку");
    }
}
