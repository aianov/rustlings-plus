// История чата читается запросом, а строки ответа надо превратить в Rust-значения. Драйвер умеет разбирать
// строку прямо в кортеж нужных типов, если типы совпадают со столбцами по порядку.
// TODO: `history` возвращает (sent_at, author, body) сообщений чата по порядку времени.

use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

async fn history(session: &Session, chat: i32) -> Result<Vec<(i64, String, String)>, BoxError> {
    let _ = (session, chat);
    todo!("запрос и rows_typed")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn rows_become_tuples() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 200, 'bob', 'два');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 100, 'ann', 'раз');
                INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 150, 'kate', 'чужое')").await;
        let got = history(&ks.session, 1).await.expect("история");
        assert_eq!(got, [(100, "ann".to_string(), "раз".to_string()), (200, "bob".to_string(), "два".to_string())], "сообщения чата 1 по порядку");
        assert!(history(&ks.session, 42).await.expect("пустой чат").is_empty(), "у пустого чата пустая история");
    }
}
