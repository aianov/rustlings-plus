// Список авторов с числом постов, страной автора и ником. База отказывается выполнять запрос и
// ругается, что столбец должен быть в GROUP BY или внутри агрегата. Нужны столбцы username,
// country, posts (число постов), порядок по нику. Авторы без постов не нужны.
// TODO: Разберись, чего хочет база, и почини запрос так, чтобы группа была по человеку.

fn authors_list() -> &'static str {
    "SELECT u.username, u.country, count(*) AS posts
     FROM users u
     JOIN posts p ON p.author_id = u.id
     GROUP BY u.username
     ORDER BY u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn grouped_by_person() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(authors_list()).await;
        assert_eq!(t.columns, ["username", "country", "posts"], "нужны столбцы username, country, posts");
        assert_eq!(t.rows, [["alice", "DE", "3"], ["bob", "US", "2"], ["carol", "DE", "2"], ["dave", "FR", "1"]], "четыре автора с постами:\n{t}");
    }
}
