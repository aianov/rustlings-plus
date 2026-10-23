// На странице автора нужен его самый залайканный пост: по одному на автора, столбцы author_id,
// id и likes, порядок по author_id. При равенстве лайков берётся более ранний пост. Запрос написан,
// но база отвечает ошибкой про оконную функцию в WHERE.
// TODO: Почини запрос.

fn top_post_per_author() -> &'static str {
    "SELECT author_id, id, likes FROM posts
     WHERE row_number() OVER (PARTITION BY author_id ORDER BY likes DESC, created_at) = 1
     ORDER BY author_id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn one_best_post_per_author() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(top_post_per_author()).await;
        assert_eq!(t.columns, ["author_id", "id", "likes"], "нужны столбцы author_id, id, likes");
        assert_eq!(
            t.rows,
            [["1", "3", "8"], ["2", "5", "8"], ["3", "4", "3"], ["4", "6", "8"], ["5", "9", "1"]],
            "у bob два поста с восемью лайками (2 и 5), нужен более ранний, это 5; по строке на автора:\n{t}"
        );
    }
}
