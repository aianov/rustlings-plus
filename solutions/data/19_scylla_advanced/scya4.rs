use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

const CAP: usize = 99;

async fn unread(session: &Session, user: i32, chat: i32) -> Result<String, BoxError> {
    let read = session.query("SELECT last_read FROM reads WHERE user_id = ? AND chat_id = ?", (user, chat)).await?;
    let last_read = read.maybe_first_row_typed::<(i64,)>()?.map_or(i64::MIN, |(t,)| t);
    let limit = i32::try_from(CAP + 1)?;
    let res = session
        .query("SELECT sent_at FROM messages WHERE chat_id = ? AND sent_at > ? LIMIT ?", (chat, last_read, limit))
        .await?;
    let n = res.rows_num()?;
    Ok(if n > CAP { format!("{CAP}+") } else { n.to_string() })
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC);
        CREATE TABLE reads (user_id int, chat_id int, last_read bigint, PRIMARY KEY (user_id, chat_id))";

    async fn ready() -> TestKs {
        let ks = TestKs::with_cql(SCHEMA).await;
        for chunk in 0..3 {
            let mut batch = String::from("BEGIN UNLOGGED BATCH ");
            for j in 0..50 {
                batch.push_str(&format!("INSERT INTO messages (chat_id, sent_at, body) VALUES (1, {}, 'm'); ", chunk * 50 + j + 1));
            }
            batch.push_str("APPLY BATCH");
            ks.session.query(batch, ()).await.expect("загрузка");
        }
        ks.run("INSERT INTO messages (chat_id, sent_at, body) VALUES (2, 10, 'a'); INSERT INTO messages (chat_id, sent_at, body) VALUES (2, 20, 'b');
                INSERT INTO reads (user_id, chat_id, last_read) VALUES (7, 1, 145);
                INSERT INTO reads (user_id, chat_id, last_read) VALUES (7, 2, 20);
                INSERT INTO reads (user_id, chat_id, last_read) VALUES (9, 1, 20)").await;
        ks
    }

    #[tokio::test]
    async fn badge_for_each_case() {
        let ks = ready().await;
        assert_eq!(unread(&ks.session, 7, 1).await.expect("значок"), "5", "после 145 пришло 5 сообщений");
        assert_eq!(unread(&ks.session, 7, 2).await.expect("значок"), "0", "всё прочитано");
        assert_eq!(unread(&ks.session, 9, 1).await.expect("значок"), "99+", "130 непрочитанных это 99+");
        assert_eq!(unread(&ks.session, 12, 2).await.expect("значок"), "2", "кто не читал, у того непрочитаны все");
    }
}
