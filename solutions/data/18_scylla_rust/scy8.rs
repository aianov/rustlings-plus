use scylla::prepared_statement::PreparedStatement;
use scylla::transport::errors::QueryError;
use scylla::Session;

struct Store {
    insert: PreparedStatement,
}

impl Store {
    async fn new(session: &Session) -> Result<Store, QueryError> {
        let insert = session.prepare("INSERT INTO messages (chat_id, sent_at, body) VALUES (?, ?, ?)").await?;
        Ok(Store { insert })
    }

    async fn send(&self, session: &Session, chat: i32, sent_at: i64, body: &str) -> Result<(), QueryError> {
        session.execute(&self.insert, (chat, sent_at, body)).await?;
        Ok(())
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SOURCE: &str = include_str!("scy8.rs");
    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn sends_work() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let store = Store::new(&ks.session).await.expect("создание хранилища");
        for i in 0..20 {
            store.send(&ks.session, 1, i, "сообщение").await.expect("отправка");
        }
        let t = ks.table("SELECT COUNT(*) FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.cell(0, 0), "20", "все отправки записаны\n{t}");
    }

    #[test]
    fn send_does_not_prepare() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        let send = code.split("async fn send").nth(1).unwrap_or_default();
        assert!(!send.contains(".prepare("), "send выполняет готовый запрос и ничего не готовит");
        assert!(code.contains(".prepare("), "запрос по-прежнему готовится, но один раз, в new");
    }
}
