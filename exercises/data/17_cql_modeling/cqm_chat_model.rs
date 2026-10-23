// Квиз главы: модель чата по списку запросов. Тест сам пишет данные и выполняет запросы, твоя часть это схема.
// Таблицы и столбцы:
// messages (chat_id int, month int, id timeuuid, author_id int, body text):
//   Q1 `WHERE chat_id = ? AND month = ? LIMIT n` отдаёт сообщения месяца свежими первыми, одновременные не теряются;
// chats_by_user (user_id int, last_at bigint, chat_id int, title text):
//   Q2 `WHERE user_id = ?` отдаёт чаты по убыванию активности, чаты с одинаковым временем не теряются;
// chat_members (chat_id int, user_id int, role text):
//   Q3 `WHERE chat_id = ? AND user_id = ?` находит одну строку членства;
// user_by_name (username text, user_id int):
//   Q4 `WHERE username = ?` находит пользователя.
// TODO: Четыре CREATE TABLE через точку с запятой.

fn schema() -> &'static str {
    ""
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO messages (chat_id, month, id, author_id, body) VALUES (1, 202610, 0b898000-bd2b-11f1-8001-1a2b3c4d5e6f, 7, 'старое');INSERT INTO messages (chat_id, month, id, author_id, body) VALUES (1, 202610, 3162c000-cc19-11f1-8002-0000000000aa, 9, 'одновременно а');INSERT INTO messages (chat_id, month, id, author_id, body) VALUES (1, 202610, 3162c000-cc19-11f1-8003-0000000000bb, 12, 'одновременно б');INSERT INTO messages (chat_id, month, id, author_id, body) VALUES (1, 202609, bccc0000-a8bc-11f1-8004-1a2b3c4d5e6f, 7, 'сентябрь');INSERT INTO chats_by_user (user_id, last_at, chat_id, title) VALUES (7, 300, 1, 'Rust');INSERT INTO chats_by_user (user_id, last_at, chat_id, title) VALUES (7, 500, 2, 'Go');INSERT INTO chats_by_user (user_id, last_at, chat_id, title) VALUES (7, 500, 3, 'Zig');INSERT INTO chat_members (chat_id, user_id, role) VALUES (1, 7, 'admin');INSERT INTO chat_members (chat_id, user_id, role) VALUES (1, 9, 'member');INSERT INTO user_by_name (username, user_id) VALUES ('kate', 7)";

    async fn ready() -> TestKs {
        assert!(!schema().trim().is_empty(), "сначала напиши схему");
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        ks
    }

    #[tokio::test]
    async fn q1_month_of_chat() {
        let ks = ready().await;
        let t = ks.table("SELECT body FROM messages WHERE chat_id = 1 AND month = 202610 LIMIT 5").await;
        assert_eq!(t.len(), 3, "в октябре три сообщения, два из них в одну миллисекунду\n{t}");
        assert_eq!(t.cell(2, 0), "старое", "свежие первыми, старое в конце\n{t}");
    }

    #[tokio::test]
    async fn q2_chats_by_activity() {
        let ks = ready().await;
        let t = ks.table("SELECT chat_id FROM chats_by_user WHERE user_id = 7").await;
        assert_eq!(t.column(0), ["2", "3", "1"], "по убыванию активности, оба чата с временем 500 на месте\n{t}");
    }

    #[tokio::test]
    async fn q3_membership() {
        let ks = ready().await;
        let t = ks.table("SELECT role FROM chat_members WHERE chat_id = 1 AND user_id = 9").await;
        assert_eq!(t.column(0), ["member"], "одна строка членства\n{t}");
    }

    #[tokio::test]
    async fn q4_user_by_name() {
        let ks = ready().await;
        let t = ks.table("SELECT user_id FROM user_by_name WHERE username = 'kate'").await;
        assert_eq!(t.column(0), ["7"], "поиск по нику\n{t}");
    }
}
