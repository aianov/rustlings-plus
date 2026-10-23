// Вставка передаёт значения кортежем, а порядок столбцов в запросе кто-то переставил (body первым), и запись
// падает с ошибкой типов. Переставить кортеж обратно можно, но в следующий раз сломается снова. Сделай так,
// чтобы значения связывались со столбцами по именам: структура NewMessage должна уметь сериализоваться как
// строка запроса, и тогда порядок столбцов в тексте запроса перестанет иметь значение.

use scylla::transport::errors::QueryError;
use scylla::Session;

struct NewMessage {
    chat_id: i32,
    sent_at: i64,
    author: String,
    body: String,
}

async fn save(session: &Session, msg: &NewMessage) -> Result<(), QueryError> {
    session
        .query("INSERT INTO messages (body, author, chat_id, sent_at) VALUES (?, ?, ?, ?)", (msg.chat_id, msg.sent_at, &msg.author, &msg.body))
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SOURCE: &str = include_str!("scy6.rs");
    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn struct_binds_by_name() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let msg = NewMessage { chat_id: 1, sent_at: 100, author: "ann".to_string(), body: "привет".to_string() };
        save(&ks.session, &msg).await.expect("вставка структуры");
        let t = ks.table("SELECT sent_at, author, body FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["100", "ann", "привет"]], "каждое поле попало в свой столбец\n{t}");
    }

    #[test]
    fn binding_is_by_name() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(code.contains("SerializeRow"), "структура должна сериализоваться сама: derive(SerializeRow)");
    }
}
