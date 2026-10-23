// Каждый запрос с параметрами драйвер сначала готовит на сервере (prepare), а потом выполняет. Для частых
// запросов это лишний круг. Правильно подготовить их один раз при создании хранилища и дальше только
// выполнять (execute). Подготовленный запрос знает типы параметров и, главное, то, какой из них ключ партиции.
// TODO: `Store::new` готовит вставку и чтение истории, `send` и `history` их выполняют.

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
        let _ = session;
        todo!("prepare двух запросов")
    }

    async fn send(&self, session: &Session, chat: i32, sent_at: i64, author: &str, body: &str) -> Result<(), QueryError> {
        let _ = (session, chat, sent_at, author, body, &self.insert);
        todo!("execute вставки")
    }

    async fn history(&self, session: &Session, chat: i32) -> Result<Vec<(i64, String)>, BoxError> {
        let _ = (session, chat, &self.history);
        todo!("execute чтения")
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
