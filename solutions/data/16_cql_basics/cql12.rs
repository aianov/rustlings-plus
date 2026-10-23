fn schema() -> &'static str {
    "CREATE TABLE chat_messages (chat_id int, sent_at bigint, body text, title text STATIC, PRIMARY KEY (chat_id, sent_at))"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    #[tokio::test]
    async fn title_is_shared_by_the_chat() {
        let ks = TestKs::with_cql(schema()).await;
        ks.run("INSERT INTO chat_messages (chat_id, title) VALUES (1, 'Rustaceans');
                INSERT INTO chat_messages (chat_id, sent_at, body) VALUES (1, 100, 'привет');
                INSERT INTO chat_messages (chat_id, sent_at, body) VALUES (1, 200, 'кто тут');
                INSERT INTO chat_messages (chat_id, title) VALUES (2, 'Go');
                INSERT INTO chat_messages (chat_id, sent_at, body) VALUES (2, 150, 'hi')").await;
        let t = ks.table("SELECT sent_at, title FROM chat_messages WHERE chat_id = 1").await;
        assert_eq!(t.rows, [["100", "Rustaceans"], ["200", "Rustaceans"]], "название чата видно из каждого сообщения\n{t}");
        ks.run("UPDATE chat_messages SET title = 'Rust' WHERE chat_id = 1").await;
        let t = ks.table("SELECT title FROM chat_messages WHERE chat_id = 1").await;
        assert_eq!(t.column(0), ["Rust", "Rust"], "переименование одной записью меняет название у всей истории\n{t}");
        let other = ks.table("SELECT title FROM chat_messages WHERE chat_id = 2").await;
        assert_eq!(other.column(0), ["Go"], "у другого чата своё название\n{other}");
    }
}
