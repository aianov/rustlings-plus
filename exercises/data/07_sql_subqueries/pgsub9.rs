// Для еженедельного письма нужны авторы, у которых лайков за всё время больше, чем в среднем
// у авторов. «Авторы» это те, у кого есть хотя бы один пост (frank и grace не пишут, в среднее
// не входят); автор с постами, но без единого лайка в среднее входит с нулём. Столбцы username и likes
// (сумма лайков на всех его постах), порядок: больше лайков выше, при равенстве по нику.
// Запрос получится длинным: разбей его на именованные шаги.
// TODO: Напиши запрос.

fn above_average_authors() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn authors_above_the_average() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(above_average_authors()).await;
        assert_eq!(t.columns, ["username", "likes"], "нужны столбцы username и likes");
        assert_eq!(
            t.rows,
            [["alice", "5"], ["dave", "2"]],
            "лайков по авторам: alice 5, dave 2, bob 1, carol 1, eve 0; среднее 1.8 (eve входит с нулём), выше него alice и dave:\n{t}"
        );
    }

    #[tokio::test]
    async fn average_moves_with_the_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO likes VALUES (10, 1), (10, 2), (10, 3), (10, 4), (10, 5), (10, 6), (10, 7)").await;
        let t = db.table(above_average_authors()).await;
        assert_eq!(
            t.rows,
            [["bob", "8"], ["alice", "5"]],
            "после семи лайков на пост bob у него 8, остальные: 5, 2, 1, 0 при среднем 3.2; выше bob и alice:\n{t}"
        );
    }
}
