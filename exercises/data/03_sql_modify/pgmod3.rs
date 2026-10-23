// После создания поста приложение должно показать его автору: нужны id и те значения, которые
// проставила сама база (число лайков по умолчанию и время создания). Делать для этого второй
// запрос SELECT нельзя: лишний круг до базы да ещё и гонка с другими вставками.
// Создай пост автора с id 1 и текстом 'hello' и верни из этого же запроса столбцы id,
// likes_count, created_at (в таком порядке).
// TODO: Напиши запрос.

fn create_post() -> &'static str {
    "INSERT INTO posts (author_id, body) VALUES (1, 'hello')"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn returns_database_filled_values() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(create_post()).await;
        assert_eq!(t.columns, ["id", "likes_count", "created_at"], "запрос должен вернуть столбцы id, likes_count, created_at");
        assert_eq!(t.len(), 1, "должна вернуться ровно одна строка:\n{t}");
        assert_eq!(t.cell(0, 0), "7", "id нового поста следует за шестью существующими:\n{t}");
        assert_eq!(t.cell(0, 1), "0", "лайков по умолчанию 0");
    }

    #[tokio::test]
    async fn the_post_is_really_stored() {
        let db = TestDb::with_sql(SEED).await;
        db.table(create_post()).await;
        let t = db.table("SELECT body FROM posts WHERE id = 7").await;
        assert_eq!(t.rows, [["hello"]], "пост должен быть сохранён");
    }
}
