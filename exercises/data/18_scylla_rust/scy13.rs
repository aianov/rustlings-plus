// Отчёт считает, сколько символов написано в чате. Чтобы он не тянул в память всю партицию, у запроса
// задан размер страницы 1000. В маленьких чатах цифра верная, а в чате на 12 000 сообщений отчёт показывает
// меньше, чем есть, и никакой ошибки. Разберись, сколько строк возвращает один execute с заданным размером
// страницы, и сделай так, чтобы отчёт проходил по всем сообщениям чата, держа в памяти не больше страницы.

use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

async fn total_chars(session: &Session, chat: i32) -> Result<usize, BoxError> {
    let mut stmt = session.prepare("SELECT body FROM messages WHERE chat_id = ?").await?;
    stmt.set_page_size(1000);
    let res = session.execute(&stmt, (chat,)).await?;
    let mut total = 0;
    for row in res.rows_typed::<(String,)>()? {
        total += row?.0.chars().count();
    }
    Ok(total)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn every_message_is_counted() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let mut expected = 0;
        for chunk in 0..240 {
            let mut batch = String::from("BEGIN UNLOGGED BATCH ");
            for j in 0..50 {
                let i = chunk * 50 + j;
                let body = "ы".repeat(i % 7 + 1);
                expected += i % 7 + 1;
                batch.push_str(&format!("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (1, {i}, 'ann', '{body}'); "));
            }
            batch.push_str("APPLY BATCH");
            ks.session.query(batch, ()).await.expect("загрузка сообщений");
        }
        ks.run("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (2, 1, 'bob', 'чужой чат')").await;
        assert_eq!(total_chars(&ks.session, 1).await.expect("отчёт"), expected, "отчёт должен пройти по всем 12000 сообщениям чата 1");
    }
}
