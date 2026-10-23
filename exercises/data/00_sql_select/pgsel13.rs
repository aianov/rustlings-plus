// Таблица лидеров постов: по лайкам, больше выше, по пять на страницу. У постов часто равное
// число лайков, и если порядок для равных не задан, Postgres вправе расставить их как удобно.
// Из-за этого один и тот же пост может попасть на обе страницы или ни на одну.
// TODO: Сделай порядок однозначным: при равных лайках раньше идёт меньший id.

fn first_page() -> &'static str {
    "SELECT id, likes FROM posts ORDER BY likes DESC LIMIT 5"
}

fn second_page() -> &'static str {
    "SELECT id, likes FROM posts ORDER BY likes DESC LIMIT 5 OFFSET 5"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn first_page_is_stable() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(first_page()).await;
        assert_eq!(t.col("id"), ["2", "4", "11", "8", "15"], "при равных лайках раньше должен идти меньший id:\n{t}");
    }

    #[tokio::test]
    async fn second_page_is_stable() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(second_page()).await;
        assert_eq!(t.col("id"), ["6", "10", "13", "1", "7"], "вторая страница продолжает первую без повторов и пропусков:\n{t}");
    }
}
