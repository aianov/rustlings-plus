// Список чатов пользователя отсортирован по последней активности: где написали последним, тот наверху.
// Таблица chats_by_activity (user_id, last_at, chat_id, title), ключ (user_id, last_at, chat_id), свежие первыми.
// Но last_at стоит в ключе, а ключ не обновляют: новое сообщение в чате это новая строка, и чат появляется в
// списке дважды. Чтобы чат оставался в списке один раз, старую строку надо удалить. Прежнее время приходит
// параметром (приложение хранит его отдельно); `None` значит, что чата в списке ещё не было.
// TODO: `touch` возвращает скрипт, после которого чат стоит в списке один раз с новым временем.

fn touch(user: i32, chat: i32, title: &str, old_at: Option<i64>, new_at: i64) -> String {
    let _ = old_at;
    format!("INSERT INTO chats_by_activity (user_id, last_at, chat_id, title) VALUES ({user}, {new_at}, {chat}, '{title}')")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE chats_by_activity (user_id int, last_at bigint, chat_id int, title text, PRIMARY KEY (user_id, last_at, chat_id)) WITH CLUSTERING ORDER BY (last_at DESC, chat_id ASC)";

    #[tokio::test]
    async fn each_chat_once_by_activity() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(&touch(7, 1, "Rust", None, 100)).await;
        ks.run(&touch(7, 2, "Go", None, 200)).await;
        ks.run(&touch(7, 1, "Rust", Some(100), 300)).await;
        ks.run(&touch(7, 3, "Zig", None, 250)).await;
        ks.run(&touch(9, 1, "Rust", None, 50)).await;
        let t = ks.table("SELECT chat_id, last_at FROM chats_by_activity WHERE user_id = 7").await;
        assert_eq!(t.rows, [["1", "300"], ["3", "250"], ["2", "200"]], "каждый чат один раз, свежая активность сверху\n{t}");
        let other = ks.table("SELECT chat_id FROM chats_by_activity WHERE user_id = 9").await;
        assert_eq!(other.column(0), ["1"], "список другого пользователя не трогаем\n{other}");
    }
}
