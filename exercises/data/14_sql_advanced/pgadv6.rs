// Страница поста собирается из пяти запросов: сам пост, автор, теги, комментарии, авторы комментариев.
// Фронтенд хочет один JSON с готовой вложенной структурой:
// {"id", "body", "author": {"id", "username"}, "tags": [...по алфавиту], "comments": [{"id", "author", "body"}]}
// где "comments" это два самых свежих комментария, свежие первыми (по времени, а не по id), а
// "author" внутри комментария это просто имя. У поста может не быть тегов или комментариев:
// тогда нужен пустой массив `[]`, а не null. Номер поста приходит как `$1`; результат один столбец `doc`.
// TODO: Допиши `post_json`.

fn post_json() -> &'static str {
    "SELECT NULL::jsonb AS doc"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use serde_json::{Value, json};

    const SEED: &str = "
        CREATE TABLE users (id bigint PRIMARY KEY, username text NOT NULL);
        CREATE TABLE posts (id bigint PRIMARY KEY, author_id bigint NOT NULL, body text NOT NULL);
        CREATE TABLE post_tags (post_id bigint NOT NULL, tag text NOT NULL);
        CREATE TABLE comments (id bigint PRIMARY KEY, post_id bigint NOT NULL, author_id bigint NOT NULL, body text NOT NULL, created_at timestamptz NOT NULL);
        INSERT INTO users VALUES (1, 'alice'), (2, 'bob'), (3, 'carol');
        INSERT INTO posts VALUES (10, 1, 'Про индексы'), (11, 2, 'Без тегов'), (12, 3, 'Одинокий пост');
        INSERT INTO post_tags VALUES (10, 'sql'), (10, 'db'), (10, 'rust'), (11, 'rust');
        INSERT INTO comments VALUES
            (1, 10, 2, 'первый', '2030-05-01 10:00+00'),
            (2, 10, 2, 'самый свежий', '2030-05-01 12:00+00'),
            (3, 10, 3, 'второй по свежести', '2030-05-01 11:00+00'),
            (4, 10, 1, 'самый старый', '2030-05-01 09:00+00'),
            (5, 12, 1, 'единственный', '2030-05-02 08:00+00');
    ";

    async fn doc(db: &TestDb, id: i64) -> Option<Value> {
        let row: Option<(Value,)> = sqlx::query_as(post_json()).bind(id).fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("запрос post_json упал: {e}"));
        row.map(|r| r.0)
    }

    #[tokio::test]
    async fn builds_the_nested_document() {
        let db = TestDb::with_sql(SEED).await;
        let got = doc(&db, 10).await.expect("пост 10 существует, строка должна быть");
        let expected = json!({
            "id": 10, "body": "Про индексы",
            "author": {"id": 1, "username": "alice"},
            "tags": ["db", "rust", "sql"],
            "comments": [
                {"id": 2, "author": "bob", "body": "самый свежий"},
                {"id": 3, "author": "carol", "body": "второй по свежести"}
            ]
        });
        assert_eq!(got, expected, "документ поста 10 собран неверно (теги по алфавиту, два свежих комментария по времени, без дублей от соединения тегов с комментариями)");
    }

    #[tokio::test]
    async fn empty_parts_are_empty_arrays() {
        let db = TestDb::with_sql(SEED).await;
        let got = doc(&db, 11).await.expect("пост 11");
        assert_eq!(got["tags"], json!(["rust"]), "у поста 11 один тег");
        assert_eq!(got["comments"], json!([]), "комментариев нет: нужен пустой массив, а не null");
        let got = doc(&db, 12).await.expect("пост 12");
        assert_eq!(got["tags"], json!([]), "тегов нет: нужен пустой массив, а не null");
        assert_eq!(got["comments"].as_array().map(|a| a.len()), Some(1), "один комментарий у поста 12");
    }

    #[tokio::test]
    async fn missing_post_gives_no_row() {
        let db = TestDb::with_sql(SEED).await;
        assert!(doc(&db, 999).await.is_none(), "поста 999 нет, строк быть не должно");
    }
}
