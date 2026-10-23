fn schema() -> &'static str {
    "CREATE TABLE chat_stats (chat_id int PRIMARY KEY, messages counter, reactions counter)"
}

fn bump() -> &'static str {
    "UPDATE chat_stats SET messages = messages + 1, reactions = reactions + 2 WHERE chat_id = 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    #[tokio::test]
    async fn counters_add_up() {
        let ks = TestKs::with_cql(schema()).await;
        for _ in 0..3 {
            ks.run(bump()).await;
        }
        let t = ks.table("SELECT messages, reactions FROM chat_stats WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["3", "6"]], "три обновления: 3 сообщения и 6 реакций\n{t}");
    }
}
