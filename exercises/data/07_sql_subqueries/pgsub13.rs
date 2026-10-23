// Теги живут в таблице tags с уникальным name. Функция «дай id тега по имени ($1), создав его, если
// нет» уже работает: оба вызова возвращают id. Но администратор базы заметил, что таблица tags
// раздулась: на каждый вызов для существующего тега в ней появляется новая версия строки (видно
// по системному столбцу xmin, он меняется). Запрос должен оставаться одним оператором и возвращать
// ровно одну строку с id.
// TODO: Найди, что пишет лишнее, и перепиши, чтобы существующая строка не менялась.

fn tag_id() -> &'static str {
    "INSERT INTO tags (name) VALUES ($1)
     ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
     RETURNING id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn get(db: &TestDb, name: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(tag_id())
            .bind(name)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("запрос не выполнился: {e}"))
            .expect("запрос должен вернуть id и для нового, и для существующего тега")
    }

    async fn version(db: &TestDb, name: &str) -> String {
        let t = db.table(&format!("SELECT xmin::text FROM tags WHERE name = '{name}'")).await;
        t.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn existing_row_is_not_rewritten() {
        let db = TestDb::with_sql(SEED).await;
        let before = version(&db, "rust").await;
        assert_eq!(get(&db, "rust").await, 1, "у rust id 1");
        assert_eq!(version(&db, "rust").await, before, "строка существующего тега переписана (xmin изменился): запрос не должен ничего менять, если тег есть");
    }

    #[tokio::test]
    async fn new_tag_is_created_once() {
        let db = TestDb::with_sql(SEED).await;
        let first = get(&db, "databases").await;
        let v = version(&db, "databases").await;
        let second = get(&db, "databases").await;
        assert_eq!(first, second, "оба вызова должны вернуть один и тот же id");
        assert_eq!(version(&db, "databases").await, v, "второй вызов не должен переписывать строку");
        assert_eq!(db.table("SELECT id FROM tags").await.len(), 4, "тегов должно стать четыре");
    }
}
