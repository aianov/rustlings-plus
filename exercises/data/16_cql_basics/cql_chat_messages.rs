// Квиз главы: таблица сообщений чата и все её запросы. Таблица `chat_messages`: chat_id int, sent_at timeuuid,
// author text, body text и название чата title, общее для чата. Внутри чата свежие сообщения первыми.
// Запросы (везде чат 7):
// 1) latest: три последних сообщения, столбцы author и body;
// 2) page_before: два сообщения, отправленные раньше сообщения с id 52c5c800-cd4f-11f1-800e-1a2b3c4d5e6f, свежие первыми, author и body;
// 3) day: тексты (body) всех сообщений за 21 октября 2026 по UTC, свежие первыми;
// 4) title: название чата одной строкой, столбец title.
// TODO: Напиши схему и четыре запроса, без ALLOW FILTERING.

fn schema() -> &'static str {
    ""
}

fn latest() -> &'static str {
    ""
}

fn page_before() -> &'static str {
    ""
}

fn day() -> &'static str {
    ""
}

fn title() -> &'static str {
    ""
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const ROWS: &str = "INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 9843b000-ccd1-11f1-800b-1a2b3c4d5e6f, 'ann', 'вечер 20-го'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 5bcc8000-cce2-11f1-800c-1a2b3c4d5e6f, 'bob', 'полночь 21-го'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, e4254200-cd2f-11f1-800d-1a2b3c4d5e6f, 'ann', 'утро'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 52c5c800-cd4f-11f1-800e-1a2b3c4d5e6f, 'kate', 'обед'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 863618f0-cdab-11f1-800f-1a2b3c4d5e6f, 'bob', 'почти 22-е'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 86364000-cdab-11f1-8010-1a2b3c4d5e6f, 'ann', 'полночь 22-го'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (7, 94598000-cdee-11f1-8011-1a2b3c4d5e6f, 'kate', 'утро 22-го'); INSERT INTO chat_messages (chat_id, sent_at, author, body) VALUES (8, f1016000-cd46-11f1-8012-1a2b3c4d5e6f, 'zed', 'чужой чат'); INSERT INTO chat_messages (chat_id, title) VALUES (7, 'Растоманы'); INSERT INTO chat_messages (chat_id, title) VALUES (8, 'Другой')";

    async fn ready() -> TestKs {
        assert!(!schema().is_empty(), "сначала напиши схему");
        let ks = TestKs::with_cql(schema()).await;
        ks.run(ROWS).await;
        for q in [latest(), page_before(), day(), title()] {
            assert!(!q.to_uppercase().contains("ALLOW FILTERING"), "без ALLOW FILTERING: `{q}`");
        }
        ks
    }

    #[tokio::test]
    async fn latest_three() {
        let ks = ready().await;
        let t = ks.table(latest()).await;
        assert_eq!(t.rows, [["kate", "утро 22-го"], ["ann", "полночь 22-го"], ["bob", "почти 22-е"]], "три самых свежих, свежие первыми\n{t}");
    }

    #[tokio::test]
    async fn page_before_cursor() {
        let ks = ready().await;
        let t = ks.table(page_before()).await;
        assert_eq!(t.rows, [["ann", "утро"], ["bob", "полночь 21-го"]], "два сообщения строго до курсора\n{t}");
    }

    #[tokio::test]
    async fn one_day() {
        let ks = ready().await;
        let t = ks.table(day()).await;
        assert_eq!(t.column(0), ["почти 22-е", "обед", "утро", "полночь 21-го"], "21 октября целиком: полночь 21-го включается, полночь 22-го нет\n{t}");
    }

    #[tokio::test]
    async fn chat_title() {
        let ks = ready().await;
        let t = ks.table(title()).await;
        assert_eq!(t.rows, [["Растоманы"]], "название чата одной строкой\n{t}");
        let kinds = ks
            .table(&format!("SELECT column_name, kind FROM system_schema.columns WHERE keyspace_name = '{}' AND table_name = 'chat_messages'", ks.name()))
            .await;
        assert!(kinds.rows.iter().any(|r| r[0] == "title" && r[1] == "static"), "название хранится один раз на чат\n{kinds}");
    }
}
