use scylla::prepared_statement::PreparedStatement;
use scylla::statement::Consistency;
use scylla::transport::errors::QueryError;
use scylla::Session;

async fn prepare_send(session: &Session) -> Result<PreparedStatement, QueryError> {
    let mut stmt = session.prepare("INSERT INTO messages (chat_id, sent_at, author, body) VALUES (?, ?, ?, ?)").await?;
    stmt.set_consistency(Consistency::Quorum);
    stmt.set_is_idempotent(true);
    Ok(stmt)
}

async fn prepare_count(session: &Session) -> Result<PreparedStatement, QueryError> {
    let mut stmt = session.prepare("UPDATE chat_stats SET messages = messages + 1 WHERE chat_id = ?").await?;
    stmt.set_consistency(Consistency::Quorum);
    stmt.set_is_idempotent(false);
    Ok(stmt)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use scylla::statement::Consistency;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, sent_at bigint, author text, body text, PRIMARY KEY (chat_id, sent_at)); CREATE TABLE chat_stats (chat_id int PRIMARY KEY, messages counter)";

    #[tokio::test]
    async fn statements_carry_their_properties() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let send = prepare_send(&ks.session).await.expect("подготовка вставки");
        let count = prepare_count(&ks.session).await.expect("подготовка счётчика");
        assert_eq!(send.get_consistency(), Some(Consistency::Quorum), "отправка идёт с QUORUM");
        assert!(send.get_is_idempotent(), "вставка с готовым ключом идемпотентна");
        assert_eq!(count.get_consistency(), Some(Consistency::Quorum), "счётчик тоже с QUORUM");
        assert!(!count.get_is_idempotent(), "прибавление счётчика нельзя повторять");
        ks.session.execute(&send, (1, 100_i64, "ann", "привет")).await.expect("вставка выполняется");
        ks.session.execute(&count, (1,)).await.expect("счётчик выполняется");
        let t = ks.table("SELECT messages FROM chat_stats WHERE chat_id = 1").await;
        assert_eq!(t.cell(0, 0), "1", "счётчик прибавился один раз\n{t}");
    }
}
