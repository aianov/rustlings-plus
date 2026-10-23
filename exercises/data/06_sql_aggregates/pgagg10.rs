// Две выгрузки для интерфейса, которые склеивают значения группы в одно поле.
//  - `tags_of_posts`: для каждого поста с тегами его id (post_id) и теги одной строкой через
//    запятую с пробелом (tags), в алфавитном порядке: `async, rust, tips`. Порядок строк по id.
//  - `post_ids_by_author`: для каждого автора ник (username) и массив id его постов от новых
//    к старым (post_ids). Порядок по нику.
// TODO: Напиши оба запроса.

fn tags_of_posts() -> &'static str {
    "SELECT 1"
}

fn post_ids_by_author() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn tags_sorted_inside_the_group() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(tags_of_posts()).await;
        assert_eq!(t.columns, ["post_id", "tags"], "нужны столбцы post_id и tags");
        assert_eq!(t.rows, [["1", "async, rust, tips"], ["3", "coffee"], ["5", "async, rust"]], "теги по алфавиту внутри поста; порядок вставки связей другой, на него полагаться нельзя:\n{t}");
    }

    #[tokio::test]
    async fn ids_newest_first() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_ids_by_author()).await;
        assert_eq!(t.columns, ["username", "post_ids"], "нужны столбцы username и post_ids");
        assert_eq!(
            t.rows,
            [["alice", "{5,2,1}"], ["bob", "{6,3}"], ["carol", "{7,4}"], ["dave", "{8}"]],
            "id постов от новых к старым по created_at, а не по id:\n{t}"
        );
    }
}
