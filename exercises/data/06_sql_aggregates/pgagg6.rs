// Список активных авторов: тех, кто написал два поста или больше, но считать нужно только свежие
// посты, опубликованные 29 сентября 2026 и позже. Нужны столбцы username и posts (число
// свежих постов), порядок по нику.
// TODO: Напиши запрос. Условие на посты и условие на число постов нужны оба, и они не
// взаимозаменяемы.

fn active_authors() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn fresh_posts_only() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(active_authors()).await;
        assert_eq!(t.columns, ["username", "posts"], "нужны столбцы username и posts");
        assert_eq!(
            t.rows,
            [["bob", "2"], ["carol", "2"]],
            "свежих постов (с 29 сентября) по два у bob и carol; у alice их всего один, остальные старые, хотя всего постов три:\n{t}"
        );
    }
}
