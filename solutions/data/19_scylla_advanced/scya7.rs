fn add_member(chat: i32, current: &[i32], user: i32) -> String {
    let _ = current;
    format!("UPDATE chats SET members = members + {{{user}}} WHERE chat_id = {chat}")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE chats (chat_id int PRIMARY KEY, title text, members set<int>)";

    #[tokio::test]
    async fn concurrent_invites_keep_everyone() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run("INSERT INTO chats (chat_id, title, members) VALUES (1, 'Rust', {1, 2})").await;
        let seen_by_both = [1, 2];
        ks.run(&add_member(1, &seen_by_both, 3)).await;
        ks.run(&add_member(1, &seen_by_both, 4)).await;
        let t = ks.table("SELECT members FROM chats WHERE chat_id = 1").await;
        assert_eq!(t.cell(0, 0), "{1, 2, 3, 4}", "оба приглашённых в чате, хотя оба клиента видели старый список\n{t}");
    }
}
