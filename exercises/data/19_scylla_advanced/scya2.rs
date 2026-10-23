// Сообщение редактируют с двух устройств. Оба прочитали версию 1, каждый отправил свою правку, и
// вторая молча затёрла первую. Нужна правка «только если версия всё ещё та, что я видел»: удачная правка
// поднимает версию на единицу и возвращает true, а опоздавшая ничего не меняет и возвращает false, чтобы
// клиент перечитал сообщение.

use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

async fn edit(session: &Session, chat: i32, sent_at: i64, seen_version: i32, body: &str) -> Result<bool, BoxError> {
    session
        .query("UPDATE messages SET body = ?, version = ? WHERE chat_id = ? AND sent_at = ?", (body, seen_version + 1, chat, sent_at))
        .await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, body text, version int, PRIMARY KEY (chat_id, sent_at))";

    #[tokio::test]
    async fn stale_edit_is_rejected() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO messages (chat_id, sent_at, body, version) VALUES (1, 100, 'черновик', 1)").await;
        assert!(edit(&ks.session, 1, 100, 1, "правка с телефона").await.expect("правка"), "первая правка по версии 1 проходит");
        assert!(!edit(&ks.session, 1, 100, 1, "правка с ноутбука").await.expect("правка"), "вторая правка видела старую версию и должна быть отвергнута");
        let t = ks.table("SELECT body, version FROM messages WHERE chat_id = 1 AND sent_at = 100").await;
        assert_eq!(t.rows, [["правка с телефона", "2"]], "осталась первая правка, версия 2\n{t}");
        assert!(edit(&ks.session, 1, 100, 2, "правка после перечитывания").await.expect("правка"), "после перечитывания правка проходит");
    }
}
