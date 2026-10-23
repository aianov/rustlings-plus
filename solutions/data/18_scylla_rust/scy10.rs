use scylla::frame::value::MaybeUnset;
use scylla::transport::errors::QueryError;
use scylla::Session;

fn keep<T>(value: Option<T>) -> MaybeUnset<T> {
    value.map_or(MaybeUnset::Unset, MaybeUnset::Set)
}

async fn edit(session: &Session, chat: i32, sent_at: i64, body: Option<&str>, pinned: Option<bool>) -> Result<(), QueryError> {
    session
        .query("UPDATE messages SET body = ?, pinned = ? WHERE chat_id = ? AND sent_at = ?", (keep(body), keep(pinned), chat, sent_at))
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, pinned boolean, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn untouched_fields_stay() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, body, pinned) VALUES (1, 100, 'текст', false)").await;
        edit(&ks.session, 1, 100, None, Some(true)).await.expect("закрепление");
        let t = ks.table("SELECT body, pinned FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.rows, [["текст", "true"]], "закрепление не трогает текст\n{t}");
        edit(&ks.session, 1, 100, Some("правка"), None).await.expect("правка текста");
        let t = ks.table("SELECT body, pinned FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.rows, [["правка", "true"]], "правка текста не трогает закрепление\n{t}");
    }
}
