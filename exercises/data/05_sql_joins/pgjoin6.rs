// В users есть столбец invited_by: id того, кто пригласил, или NULL, если человек пришёл сам.
// Для страницы «приглашения» нужны ник и ник пригласившего (столбцы username и invited_by).
// Пришедшие сами тоже должны быть в списке, с пустым значением. Порядок по id пользователя.
// TODO: Напиши запрос: таблица здесь соединяется сама с собой.

fn invitations() -> &'static str {
    "SELECT username, invited_by FROM users ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn names_of_inviters() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(invitations()).await;
        assert_eq!(t.columns, ["username", "invited_by"], "нужны столбцы username и invited_by");
        assert_eq!(
            t.rows,
            [["alice", "NULL"], ["bob", "alice"], ["carol", "alice"], ["dave", "bob"], ["eve", "carol"], ["frank", "alice"], ["grace", "NULL"]],
            "вместо числа должен быть ник пригласившего, а alice и grace остаются с NULL:\n{t}"
        );
    }
}
