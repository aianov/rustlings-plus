use futures::{StreamExt, TryStreamExt, stream};
use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

const IN_FLIGHT: usize = 32;

async fn import(session: &Session, rows: &[(i32, i64, String)]) -> Result<(), BoxError> {
    let insert = session.prepare("INSERT INTO messages (chat_id, sent_at, body) VALUES (?, ?, ?)").await?;
    let insert = &insert;
    stream::iter(rows)
        .map(|(chat, sent_at, body)| async move { session.execute(insert, (chat, sent_at, body)).await })
        .buffer_unordered(IN_FLIGHT)
        .try_for_each(|_| async { Ok(()) })
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SOURCE: &str = include_str!("scya9.rs");
    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn two_thousand_rows_arrive() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let body = "старое сообщение ".repeat(40);
        let rows: Vec<(i32, i64, String)> = (0..2000).map(|i| (i % 400, i64::from(i), body.clone())).collect();
        import(&ks.session, &rows).await.expect("импорт двух тысяч сообщений");
        let t = ks.table("SELECT COUNT(*) FROM messages").await;
        assert_eq!(t.cell(0, 0), "2000", "все сообщения импортированы\n{t}");
    }

    #[test]
    fn no_batches_for_bulk_load() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.to_uppercase().contains("BATCH"), "массовая загрузка идёт отдельными подготовленными вставками, без batch");
    }
}
