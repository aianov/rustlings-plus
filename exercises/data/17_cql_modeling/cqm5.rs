// История чата режется по месяцам: ключ партиции (chat_id, month), где month это YYYYMM по UTC, например
// 202610. Номер месяца считает приложение из времени отправки в миллисекундах, и считает его одинаково
// на всех серверах, в каком бы часовом поясе они ни стояли.
// TODO: Напиши `month_bucket` и `insert`, который пишет сообщение в таблицу messages (chat_id, month,
// sent_at, body) с правильным бакетом.

fn month_bucket(sent_at_ms: i64) -> i32 {
    (sent_at_ms / (30 * 24 * 3600 * 1000)) as i32
}

fn insert(chat: i32, sent_at_ms: i64, body: &str) -> String {
    format!("INSERT INTO messages (chat_id, month, sent_at, body) VALUES ({chat}, 0, {sent_at_ms}, '{body}')")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE messages (chat_id int, month int, sent_at bigint, body text, PRIMARY KEY ((chat_id, month), sent_at)) WITH CLUSTERING ORDER BY (sent_at DESC)";

    #[test]
    fn month_boundaries_in_utc() {
        for (ms, want) in [(1793491199999, 202610), (1793491200000, 202611), (1798761540000, 202612), (1798761600000, 202701), (1835438400000, 202802), (0, 197001)] {
            assert_eq!(month_bucket(ms), want, "бакет для {ms} мс должен быть {want}");
        }
    }

    #[tokio::test]
    async fn messages_land_in_their_months() {
        let ks = TestKs::with_cql(SCHEMA).await;
        ks.run(&insert(1, 1793491199999, "конец октября")).await;
        ks.run(&insert(1, 1793491200000, "начало ноября")).await;
        let oct = ks.table("SELECT body FROM messages WHERE chat_id = 1 AND month = 202610").await;
        assert_eq!(oct.column(0), ["конец октября"], "последняя миллисекунда октября остаётся в октябре\n{oct}");
        let nov = ks.table("SELECT body FROM messages WHERE chat_id = 1 AND month = 202611").await;
        assert_eq!(nov.column(0), ["начало ноября"], "полночь 1 ноября по UTC это уже ноябрь\n{nov}");
    }
}
