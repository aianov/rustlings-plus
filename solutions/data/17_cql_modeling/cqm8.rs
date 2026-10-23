fn touch(user: i32, chat: i32, title: &str, old_at: Option<i64>, new_at: i64) -> String {
    let title = title.replace('\'', "''");
    let insert = format!("INSERT INTO chats_by_activity (user_id, last_at, chat_id, title) VALUES ({user}, {new_at}, {chat}, '{title}')");
    match old_at {
        Some(old) => format!("DELETE FROM chats_by_activity WHERE user_id = {user} AND last_at = {old} AND chat_id = {chat}; {insert}"),
        None => insert,
    }
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
