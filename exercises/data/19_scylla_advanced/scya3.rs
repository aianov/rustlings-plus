// Нужна статистика чата: сколько сообщений и реакций. Для этого в Scylla есть счётчики. Схема ниже не
// создаётся, а скрипт обновления у коллеги падал бы, даже если бы создалась. Разберись с правилами
// counter-столбцов и почини оба: таблица chat_stats с ключом chat_id и счётчиками messages и reactions
// (название чата в этой таблице не храни), обновление прибавляет одно сообщение и две реакции чату 1.

fn schema() -> &'static str {
    "CREATE TABLE chat_stats (chat_id int PRIMARY KEY, title text, messages counter, reactions counter)"
}

fn bump() -> &'static str {
    "INSERT INTO chat_stats (chat_id, messages, reactions) VALUES (1, 1, 2)"
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
