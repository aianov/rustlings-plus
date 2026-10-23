use bytes::Bytes;
use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

async fn first_page(session: &Session, chat: i32, size: i32) -> Result<(Vec<i64>, Option<Bytes>), BoxError> {
    let mut stmt = session.prepare("SELECT sent_at FROM messages WHERE chat_id = ?").await?;
    stmt.set_page_size(size);
    let res = session.execute_paged(&stmt, (chat,), None).await?;
    let state = res.paging_state.clone();
    let rows = res.rows_typed::<(i64,)>()?.map(|r| r.map(|(t,)| t)).collect::<Result<Vec<_>, _>>()?;
    Ok((rows, state))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";
    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 1, 'ann', 'сообщение 1');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 2, 'ann', 'сообщение 2');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 3, 'ann', 'сообщение 3');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 4, 'ann', 'сообщение 4');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 5, 'ann', 'сообщение 5');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 6, 'ann', 'сообщение 6');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 7, 'ann', 'сообщение 7');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 8, 'ann', 'сообщение 8');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 9, 'ann', 'сообщение 9');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 10, 'ann', 'сообщение 10');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 11, 'ann', 'сообщение 11');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 12, 'ann', 'сообщение 12');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 13, 'ann', 'сообщение 13');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 14, 'ann', 'сообщение 14');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 15, 'ann', 'сообщение 15');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 16, 'ann', 'сообщение 16');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 17, 'ann', 'сообщение 17');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 18, 'ann', 'сообщение 18');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 19, 'ann', 'сообщение 19');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 20, 'ann', 'сообщение 20');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 21, 'ann', 'сообщение 21');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 22, 'ann', 'сообщение 22');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 23, 'ann', 'сообщение 23');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 24, 'ann', 'сообщение 24');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 25, 'ann', 'сообщение 25')";

    #[tokio::test]
    async fn page_comes_with_a_state() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let (rows, state) = first_page(&ks.session, 1, 10).await.expect("первая страница");
        assert_eq!(rows, (1..=10).collect::<Vec<i64>>(), "первые десять сообщений");
        assert!(state.is_some(), "после страницы есть продолжение: состояние пагинации");
        let (all, end) = first_page(&ks.session, 1, 30).await.expect("большая страница");
        assert_eq!(all.len(), 25, "страница больше чата отдаёт все 25");
        assert!(end.is_none(), "когда больше ничего нет, состояния нет");
    }
}
