use bytes::Bytes;
use scylla::prepared_statement::PreparedStatement;
use scylla::transport::errors::QueryError;
use scylla::Session;
use std::error::Error;
use std::fmt::Write;

type BoxError = Box<dyn Error + Send + Sync>;

const PAGE: i32 = 10;

fn encode(state: &[u8]) -> String {
    let mut out = String::with_capacity(state.len() * 2);
    for b in state {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn decode(cursor: &str) -> Option<Bytes> {
    if cursor.is_empty() || cursor.len() % 2 != 0 {
        return None;
    }
    (0..cursor.len())
        .step_by(2)
        .map(|i| cursor.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect::<Option<Vec<u8>>>()
        .map(Bytes::from)
}

async fn prepare_page(session: &Session) -> Result<PreparedStatement, QueryError> {
    let mut stmt = session.prepare("SELECT body FROM messages WHERE chat_id = ?").await?;
    stmt.set_page_size(PAGE);
    Ok(stmt)
}

async fn page(session: &Session, stmt: &PreparedStatement, chat: i32, cursor: Option<&str>) -> Result<(Vec<String>, Option<String>), BoxError> {
    let state = match cursor {
        Some(c) => Some(decode(c).ok_or("курсор испорчен")?),
        None => None,
    };
    let res = session.execute_paged(stmt, (chat,), state).await?;
    let next = res.paging_state.as_deref().map(encode);
    let bodies = res.rows_typed::<(String,)>()?.map(|r| r.map(|(b,)| b)).collect::<Result<Vec<_>, _>>()?;
    Ok((bodies, next))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";
    const ROWS: &str = "INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 1, 'ann', 'сообщение 1');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 2, 'ann', 'сообщение 2');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 3, 'ann', 'сообщение 3');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 4, 'ann', 'сообщение 4');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 5, 'ann', 'сообщение 5');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 6, 'ann', 'сообщение 6');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 7, 'ann', 'сообщение 7');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 8, 'ann', 'сообщение 8');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 9, 'ann', 'сообщение 9');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 10, 'ann', 'сообщение 10');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 11, 'ann', 'сообщение 11');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 12, 'ann', 'сообщение 12');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 13, 'ann', 'сообщение 13');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 14, 'ann', 'сообщение 14');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 15, 'ann', 'сообщение 15');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 16, 'ann', 'сообщение 16');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 17, 'ann', 'сообщение 17');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 18, 'ann', 'сообщение 18');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 19, 'ann', 'сообщение 19');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 20, 'ann', 'сообщение 20');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 21, 'ann', 'сообщение 21');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 22, 'ann', 'сообщение 22');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 23, 'ann', 'сообщение 23');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 24, 'ann', 'сообщение 24');INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, 25, 'ann', 'сообщение 25')";

    #[test]
    fn cursor_round_trip() {
        let bytes = [0u8, 1, 0xab, 0xff, 0x10];
        let text = encode(&bytes);
        assert!(text.chars().all(|c| c.is_ascii_hexdigit()), "курсор это шестнадцатеричная строка: {text}");
        assert_eq!(decode(&text).as_deref(), Some(&bytes[..]), "декодирование возвращает те же байты");
        for bad in ["", "abc", "zz", "0g"] {
            assert!(decode(bad).is_none(), "{bad:?} это не курсор");
        }
    }

    #[tokio::test]
    async fn walk_all_pages() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let stmt = prepare_page(&ks.session).await.expect("подготовка");
        let mut cursor: Option<String> = None;
        let mut sizes = Vec::new();
        let mut all = Vec::new();
        loop {
            let (bodies, next) = page(&ks.session, &stmt, 1, cursor.as_deref()).await.expect("страница");
            sizes.push(bodies.len());
            all.extend(bodies);
            match next {
                Some(n) => cursor = Some(n),
                None => break,
            }
            assert!(sizes.len() < 10, "страницы не кончаются");
        }
        assert_eq!(sizes, [10, 10, 5], "25 сообщений это страницы по 10, 10 и 5");
        let expected: Vec<String> = (1..=25).map(|i| format!("сообщение {i}")).collect();
        assert_eq!(all, expected, "все сообщения по порядку, без повторов и пропусков");
    }

    #[tokio::test]
    async fn bad_cursor_is_an_error() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(ROWS).await;
        let stmt = prepare_page(&ks.session).await.expect("подготовка");
        assert!(page(&ks.session, &stmt, 1, Some("не курсор")).await.is_err(), "испорченный курсор это ошибка запроса");
    }
}
