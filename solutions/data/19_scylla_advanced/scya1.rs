use scylla::Session;
use std::error::Error;

type BoxError = Box<dyn Error + Send + Sync>;

async fn open_dm(session: &Session, a: i32, b: i32, candidate: i32) -> Result<i32, BoxError> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let res = session
        .query("INSERT INTO direct_chats (user_a, user_b, chat_id) VALUES (?, ?, ?) IF NOT EXISTS", (lo, hi, candidate))
        .await?;
    let chat_pos = res.col_specs.iter().position(|c| c.name == "chat_id");
    let row = res.first_row()?;
    let applied = row.columns.first().and_then(|c| c.as_ref()).and_then(|v| v.as_boolean()).ok_or("нет [applied] в ответе")?;
    if applied {
        return Ok(candidate);
    }
    let pos = chat_pos.ok_or("нет chat_id в ответе")?;
    let chat = row.columns.get(pos).and_then(|c| c.as_ref()).and_then(|v| v.as_int()).ok_or("chat_id пуст")?;
    Ok(chat)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE direct_chats (user_a int, user_b int, chat_id int, PRIMARY KEY ((user_a, user_b)))";

    #[tokio::test]
    async fn one_chat_per_pair_even_in_a_race() {
        let ks = std::sync::Arc::new(TestKs::with_cql(SCHEMA).await);
        let mut tasks = Vec::new();
        for i in 0..10 {
            let ks = std::sync::Arc::clone(&ks);
            let (a, b) = if i % 2 == 0 { (7, 9) } else { (9, 7) };
            tasks.push(tokio::spawn(async move { open_dm(&ks.session, a, b, 100 + i).await.map_err(|e| e.to_string()) }));
        }
        let mut chats = Vec::new();
        for t in tasks {
            chats.push(t.await.expect("задача").expect("open_dm"));
        }
        chats.dedup();
        assert_eq!(chats.len(), 1, "все десять вызовов должны получить один чат: {chats:?}");
        let t = ks.table("SELECT chat_id FROM direct_chats").await;
        assert_eq!(t.len(), 1, "в таблице одна пара\n{t}");
        assert_eq!(t.cell(0, 0), chats[0].to_string(), "в таблице тот же чат, что вернулся всем\n{t}");
        assert_eq!(open_dm(&ks.session, 9, 7, 555).await.expect("повтор"), chats[0], "повторное открытие возвращает существующий чат");
    }
}
