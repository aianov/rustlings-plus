// Теги поста лежат прямо в строке: столбец `tags text[]`. Нужны три вещи. Посты, у которых есть ВСЕ
// теги из списка `$1`. Посты, у которых есть ХОТЯ БЫ ОДИН тег из списка `$1`. И топ-3 тегов по
// числу постов (пост считается один раз, даже если автор написал один тег дважды), при равенстве
// тегов по алфавиту. У некоторых постов теги пусты или вообще NULL.
// TODO: Допиши `with_all` и `with_any` (столбец `id` по возрастанию, список приходит как `text[]`) и
// `popular_tags` (столбцы `tag`, `n`).

fn with_all() -> &'static str {
    "SELECT 1"
}

fn with_any() -> &'static str {
    "SELECT 1"
}

fn popular_tags() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "
        CREATE TABLE posts (id bigint PRIMARY KEY, tags text[]);
        INSERT INTO posts VALUES
            (1, '{rust,db}'), (2, '{rust,rust,sql}'), (3, '{db}'), (4, '{}'), (5, NULL), (6, '{sql,db,rust}');
    ";

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn all_tags_must_be_present() {
        let db = TestDb::with_sql(SEED).await;
        let ids: Vec<i64> = sqlx::query_scalar(with_all()).bind(list(&["rust", "db"])).fetch_all(&db.pool).await.expect("with_all");
        assert_eq!(ids, vec![1, 6], "оба тега rust и db есть у постов 1 и 6");
        let ids: Vec<i64> = sqlx::query_scalar(with_all()).bind(list(&["sql"])).fetch_all(&db.pool).await.expect("with_all");
        assert_eq!(ids, vec![2, 6], "один тег sql: посты 2 и 6");
    }

    #[tokio::test]
    async fn any_tag_is_enough() {
        let db = TestDb::with_sql(SEED).await;
        let ids: Vec<i64> = sqlx::query_scalar(with_any()).bind(list(&["sql", "go"])).fetch_all(&db.pool).await.expect("with_any");
        assert_eq!(ids, vec![2, 6], "тега go нет нигде, sql есть у постов 2 и 6");
        let ids: Vec<i64> = sqlx::query_scalar(with_any()).bind(list(&["db", "sql"])).fetch_all(&db.pool).await.expect("with_any");
        assert_eq!(ids, vec![1, 2, 3, 6], "посты с db или sql, без повторов");
    }

    #[tokio::test]
    async fn tag_popularity_counts_posts_not_mentions() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(popular_tags()).await;
        let expected: Vec<Vec<&str>> = vec![vec!["db", "3"], vec!["rust", "3"], vec!["sql", "2"]];
        assert_eq!(t.rows, expected, "rust встречается в 3 постах (в посте 2 дважды, но пост один), при равенстве по алфавиту:\n{t}");
    }
}
