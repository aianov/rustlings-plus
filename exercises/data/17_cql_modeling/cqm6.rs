// Окно чата открывается с «последних N сообщений», но история лежит по месяцам, и в текущем месяце их может
// быть меньше N или вообще ноль (чат молчал). Значит, читаем месяц за месяцем назад, пока не наберём N или не
// упрёмся в предел месяцев, и у каждого месяца просим ровно столько, сколько ещё не хватает.
// TODO: `prev_month` (январь откатывается в декабрь прошлого года), `page_query` (запрос к одному месяцу)
// и `latest`, где `fetch(month, limit)` возвращает тексты одного месяца, свежие первыми.

fn prev_month(month: i32) -> i32 {
    month - 1
}

fn page_query(chat: i32, month: i32, limit: usize) -> String {
    format!("SELECT body FROM messages WHERE chat_id = {chat} AND month = {month}")
}

fn latest(mut fetch: impl FnMut(i32, usize) -> Vec<String>, from: i32, n: usize, max_months: usize) -> Vec<String> {
    let _ = (max_months, n);
    fetch(from, n)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    use std::collections::HashMap;

    #[test]
    fn previous_month_crosses_years() {
        assert_eq!(prev_month(202610), 202609);
        assert_eq!(prev_month(202601), 202512, "январь откатывается в декабрь прошлого года");
        assert_eq!(prev_month(202512), 202511);
    }

    fn history() -> HashMap<i32, Vec<String>> {
        let mut h = HashMap::new();
        h.insert(202602, vec!["февраль 2".to_string(), "февраль 1".to_string()]);
        h.insert(202512, (1..=5).rev().map(|i| format!("декабрь {i}")).collect());
        h.insert(202511, vec!["ноябрь".to_string()]);
        h
    }

    #[test]
    fn collects_across_empty_months_and_years() {
        let h = history();
        let mut calls = Vec::new();
        let got = latest(|m, limit| { calls.push((m, limit)); h.get(&m).map(|v| v.iter().take(limit).cloned().collect()).unwrap_or_default() }, 202602, 4, 12);
        assert_eq!(got, ["февраль 2", "февраль 1", "декабрь 5", "декабрь 4"], "сначала свежий месяц, потом назад через пустой январь в прошлый год");
        assert_eq!(calls, [(202602, 4), (202601, 2), (202512, 2)], "у каждого месяца просим только недостающее и останавливаемся, когда набрали");
    }

    #[test]
    fn stops_at_the_month_limit() {
        let h = history();
        let mut calls = 0;
        let got = latest(|m, limit| { calls += 1; h.get(&m).map(|v| v.iter().take(limit).cloned().collect()).unwrap_or_default() }, 202610, 3, 4);
        assert!(got.is_empty(), "за четыре месяца назад сообщений нет: {got:?}");
        assert_eq!(calls, 4, "дальше предела месяцев не читаем");
    }

    #[tokio::test]
    async fn page_query_reads_one_month() {
        let ks = TestKs::with_cql("CREATE TABLE messages (chat_id int, month int, sent_at bigint, body text, PRIMARY KEY ((chat_id, month), sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)").await;
        ks.run("INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202610, 1, 'старое');
                INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202610, 2, 'среднее');
                INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202610, 3, 'свежее');
                INSERT INTO messages (chat_id, month, sent_at, body) VALUES (1, 202609, 4, 'другой месяц')").await;
        let t = ks.table(&page_query(1, 202610, 2)).await;
        assert_eq!(t.column(0), ["свежее", "среднее"], "запрос к месяцу отдаёт не больше limit самых свежих\n{t}");
    }
}
