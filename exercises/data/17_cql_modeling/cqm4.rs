// Участники чата нужны трём экранам: Q1 список участников чата, Q2 «состоит ли пользователь в чате»
// (проверка перед каждой отправкой), Q3 список чатов пользователя. В групповом чате десятки тысяч участников,
// поэтому Q2 обязан находить одну строку, а не читать список. Таблицы: chat_members (chat_id int,
// user_id int, role text) и user_chats (user_id int, chat_id int, role text).
// TODO: Напиши схему обеих таблиц и `join`, который записывает вступление в обе.

fn schema() -> &'static str {
    "CREATE TABLE chat_members (chat_id int, user_id int, role text, PRIMARY KEY (user_id, chat_id))"
}

fn join(chat: i32, user: i32, role: &str) -> String {
    format!("INSERT INTO chat_members (chat_id, user_id, role) VALUES ({chat}, {user}, '{role}')")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    async fn ready() -> TestKs {
        let ks = TestKs::with_cql(schema()).await;
        for (chat, user, role) in [(1, 7, "admin"), (1, 9, "member"), (1, 12, "member"), (2, 7, "member"), (3, 12, "admin")] {
            ks.run(&join(chat, user, role)).await;
        }
        ks
    }

    #[tokio::test]
    async fn q1_members_of_chat() {
        let ks = ready().await;
        let t = ks.table("SELECT user_id, role FROM chat_members WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["7", "admin"], ["9", "member"], ["12", "member"]], "Q1: участники чата 1 по порядку id\n{t}");
    }

    #[tokio::test]
    async fn q2_membership_is_one_row() {
        let ks = ready().await;
        let yes = ks.table("SELECT role FROM chat_members WHERE chat_id = 1 AND user_id = 9").await;
        assert_eq!(yes.column(0), ["member"], "Q2: пользователь 9 состоит в чате 1\n{yes}");
        let no = ks.table("SELECT role FROM chat_members WHERE chat_id = 2 AND user_id = 9").await;
        assert!(no.is_empty(), "Q2: пользователь 9 не в чате 2\n{no}");
    }

    #[tokio::test]
    async fn q3_chats_of_user() {
        let ks = ready().await;
        let t = ks.table("SELECT chat_id, role FROM user_chats WHERE user_id = 7").await;
        assert_eq!(t.rows, [["1", "admin"], ["2", "member"]], "Q3: чаты пользователя 7\n{t}");
    }
}
