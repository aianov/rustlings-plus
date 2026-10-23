// Доля лайков от просмотров: сколько лайков приходится на один просмотр поста. Запрос
// аккуратный, округляет до трёх знаков, но для любого поста честно выдаёт 0.
// TODO: Найди, почему деление ничего не оставляет, и исправь.

fn like_rate() -> &'static str {
    "SELECT id, ROUND(likes / views, 3) AS rate FROM posts WHERE views > 0 ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::{Table, TestDb};

    const SEED: &str = include_str!("seed.sql");

    fn row<'a>(t: &'a Table, key: &str) -> &'a Vec<String> {
        t.rows
            .iter()
            .find(|r| r[0] == key)
            .unwrap_or_else(|| panic!("в результате нет строки с ключом {key}:\n{t}"))
    }

    #[tokio::test]
    async fn rate_is_fractional() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(like_rate()).await;
        assert_eq!(row(&t, "2")[1], "0.250", "у поста 2: 30 лайков на 120 просмотров, это 0.250:\n{t}");
        assert_eq!(row(&t, "7")[1], "0.139", "у поста 7: 250 лайков на 1800 просмотров, это 0.139:\n{t}");
    }
}
