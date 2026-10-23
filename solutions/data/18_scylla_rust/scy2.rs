use scylla::transport::errors::QueryError;
use scylla::Session;

async fn post(session: &Session, chat: i32, sent_at: i64, author: &str, body: &str) -> Result<(), QueryError> {
    session
        .query("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (?, ?, ?, ?)", (chat, sent_at, author, body))
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SOURCE: &str = include_str!("scy2.rs");
    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn any_text_is_stored_as_is() {
        let ks = TestKs::with_cql(SCHEMA).await;
        post(&ks.session, 1, 100, "o'neil", "it's fine; DROP TABLE messages").await.expect("вставка с апострофом");
        post(&ks.session, 1, 200, "bob", "обычный текст").await.expect("обычная вставка");
        let t = ks.table("SELECT author, body FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["o'neil", "it's fine; DROP TABLE messages"], ["bob", "обычный текст"]], "текст сохраняется как есть\n{t}");
    }

    #[test]
    fn values_are_bound_not_glued() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.contains("format!"), "значения уходят параметрами `?`, а не вклеиваются в текст запроса");
    }
}
