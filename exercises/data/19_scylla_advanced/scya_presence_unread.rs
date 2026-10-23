// Квиз главы: четыре приёма в одной модели чата.
// 1) `dm_insert(a, b, chat)`: CQL, который создаёт личный чат пары в direct_chats (user_a, user_b, chat_id),
//    только если его ещё нет; пара в любом порядке это одна строка.
// 2) `ephemeral(chat, sent_at, body, ttl)`: CQL исчезающего сообщения в messages (chat_id, sent_at, body).
// 3) `mark_read(user, chat, at)` и `unread(session, user, chat)`: отметка «прочитано до» в reads (user_id,
//    chat_id, last_read) и значок непрочитанного (до 99, дальше «99+»), как в задаче главы.
// 4) `edit(chat, sent_at, body, at_us)`: CQL правки текста, где побеждает более поздняя правка, а не последняя дошедшая.
// TODO: Напиши все функции.

use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

fn dm_insert(a: i32, b: i32, chat: i32) -> String {
    let _ = (a, b, chat);
    String::new()
}

fn ephemeral(chat: i32, sent_at: i64, body: &str, ttl: u32) -> String {
    let _ = (chat, sent_at, body, ttl);
    String::new()
}

fn mark_read(user: i32, chat: i32, at: i64) -> String {
    let _ = (user, chat, at);
    String::new()
}

async fn unread(session: &Session, user: i32, chat: i32) -> Result<String, BoxError> {
    let _ = (session, user, chat);
    Ok(String::new())
}

fn edit(chat: i32, sent_at: i64, body: &str, at_us: i64) -> String {
    let _ = (chat, sent_at, body, at_us);
    String::new()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use std::time::Duration;

    const SCHEMA: &str = "CREATE TABLE direct_chats (user_a int, user_b int, chat_id int, PRIMARY KEY ((user_a, user_b)));
        CREATE TABLE messages (chat_id int, sent_at bigint, body text, PRIMARY KEY (chat_id, sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC);
        CREATE TABLE reads (user_id int, chat_id int, last_read bigint, PRIMARY KEY (user_id, chat_id))";

    #[tokio::test]
    async fn direct_chat_is_created_once() {
        let ks = TestKs::with_cql(SCHEMA).await;
        assert!(!dm_insert(7, 9, 1).is_empty(), "сначала напиши dm_insert");
        ks.session.query(dm_insert(7, 9, 1), ()).await.expect("первое создание");
        ks.session.query(dm_insert(9, 7, 2), ()).await.expect("второе создание не падает");
        let t = ks.table("SELECT user_a, user_b, chat_id FROM direct_chats").await;
        assert_eq!(t.rows, [["7", "9", "1"]], "одна пара, первый чат\n{t}");
    }

    #[tokio::test]
    async fn ephemeral_message_expires() {
        let ks = TestKs::with_cql(SCHEMA).await;
        assert!(!ephemeral(1, 1, "x", 1).is_empty(), "сначала напиши ephemeral");
        ks.run(&ephemeral(1, 100, "it's secret", 1)).await;
        ks.run(&ephemeral(1, 200, "долгое", 600)).await;
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let t = ks.table("SELECT body FROM messages WHERE chat_id = 1").await;
        assert_eq!(t.column(0), ["долгое"], "короткое сообщение исчезло само\n{t}");
    }

    #[tokio::test]
    async fn unread_badge() {
        let ks = TestKs::with_cql(SCHEMA).await;
        for i in 1..=120 {
            ks.run(&format!("INSERT INTO messages (chat_id, sent_at, body) VALUES (5, {i}, 'm')")).await;
        }
        assert!(!mark_read(1, 1, 1).is_empty(), "сначала напиши mark_read");
        ks.run(&mark_read(7, 5, 117)).await;
        assert_eq!(unread(&ks.session, 7, 5).await.expect("значок"), "3", "после 117 три сообщения");
        assert_eq!(unread(&ks.session, 9, 5).await.expect("значок"), "99+", "кто не читал, у того 99+");
        ks.run(&mark_read(9, 5, 120)).await;
        assert_eq!(unread(&ks.session, 9, 5).await.expect("значок"), "0", "всё прочитано");
    }

    #[tokio::test]
    async fn later_edit_wins() {
        let ks = TestKs::with_cql(SCHEMA).await;
        assert!(!edit(1, 1, "x", 1).is_empty(), "сначала напиши edit");
        ks.run("INSERT INTO messages (chat_id, sent_at, body) VALUES (3, 100, 'черновик') USING TIMESTAMP 1000").await;
        ks.run(&edit(3, 100, "итог", 3000)).await;
        ks.run(&edit(3, 100, "промежуточное", 2000)).await;
        let t = ks.table("SELECT body FROM messages WHERE chat_id = 3 AND sent_at = 100").await;
        assert_eq!(t.column(0), ["итог"], "побеждает поздняя правка\n{t}");
    }
}
