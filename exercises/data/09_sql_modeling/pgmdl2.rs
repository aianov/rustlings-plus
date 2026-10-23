// У поста бывает несколько тегов. Предыдущий автор хранил их строкой через запятую
// ('rust,sql'), и поиск `LIKE '%sql%'` находит ещё и 'nosql'. Таблица posts (id, body) уже есть.
// Нужны таблицы tags (id, name: уникальное имя тега) и post_tags (post_id, tag_id), которые не
// допускают дублей пары и висящих ссылок и убираются вместе с постом или тегом. Запрос
// `posts_with_tag` ($1 имя тега) возвращает id постов с этим тегом по возрастанию.
// TODO: Напиши скрипт создания таблиц и запрос.

fn tags_schema() -> &'static str {
    "SELECT 1"
}

fn posts_with_tag() -> &'static str {
    "SELECT id FROM posts WHERE body LIKE '%' || $1 || '%'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn expect_error(db: &TestDb, sql: &str, code: &str, why: &str) {
        let err = db
            .fails(sql)
            .await
            .unwrap_or_else(|| panic!("{why}: Postgres принял команду, а должен был отвергнуть:\n{sql}"));
        assert_eq!(err.code, code, "{why}: ошибка не та, что ждали (SQLSTATE {code}). База сказала: {}", err.message);
    }

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL); INSERT INTO posts (body) VALUES ('a'), ('b'), ('c')").await;
        db.run(tags_schema()).await;
        db.run("INSERT INTO tags (name) VALUES ('rust'), ('sql'), ('nosql'); INSERT INTO post_tags (post_id, tag_id) VALUES (1, 1), (1, 2), (2, 3), (3, 2), (3, 3)").await;
        db
    }

    async fn with_tag(db: &TestDb, tag: &str) -> Vec<i64> {
        sqlx::query_scalar::<_, i64>(posts_with_tag())
            .bind(tag)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("posts_with_tag({tag}): {e}"))
    }

    #[tokio::test]
    async fn search_by_exact_tag() {
        let db = prepared().await;
        assert_eq!(with_tag(&db, "sql").await, [1, 3], "тег sql у постов 1 и 3; nosql это другой тег, посты с ним не подходят");
        assert_eq!(with_tag(&db, "nosql").await, [2, 3], "тег nosql у постов 2 и 3");
        assert_eq!(with_tag(&db, "rust").await, [1], "тег rust только у поста 1");
        assert!(with_tag(&db, "go").await.is_empty(), "такого тега нет");
    }

    #[tokio::test]
    async fn links_are_protected() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO post_tags (post_id, tag_id) VALUES (1, 1)", "23505", "повторная пара").await;
        expect_error(&db, "INSERT INTO post_tags (post_id, tag_id) VALUES (1, 99)", "23503", "несуществующий тег").await;
        expect_error(&db, "INSERT INTO post_tags (post_id, tag_id) VALUES (99, 1)", "23503", "несуществующий пост").await;
        expect_error(&db, "INSERT INTO tags (name) VALUES ('rust')", "23505", "повторное имя тега").await;
    }

    #[tokio::test]
    async fn links_go_away_with_their_owners() {
        let db = prepared().await;
        db.run("DELETE FROM posts WHERE id = 3").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM post_tags").await, "3", "связи удалённого поста 3 должны исчезнуть");
        db.run("DELETE FROM tags WHERE name = 'sql'").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM post_tags").await, "2", "связи удалённого тега sql должны исчезнуть");
    }
    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }
}
