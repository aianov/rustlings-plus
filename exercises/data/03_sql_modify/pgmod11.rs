// Теги постов. Функция «получить id тега по имени ($1), создав тег, если его нет» нужна
// каждому, кто вешает тег на пост. Приложение вызывает запрос и ждёт ровно одну строку с id,
// новый тег или уже существующий. Сейчас при первом вызове всё хорошо, а при втором с тем же именем
// приложение падает: строк в ответе нет. Запрос должен оставаться одним оператором (без
// отдельной проверки существования) и не создавать дублей при гонке.
// TODO: Найди причину и исправь.

fn tag_id() -> &'static str {
    "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO NOTHING RETURNING id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    async fn get(db: &TestDb, name: &str) -> Option<i64> {
        sqlx::query_scalar::<_, i64>(tag_id())
            .bind(name)
            .fetch_optional(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("запрос не выполнился: {e}"))
    }

    #[tokio::test]
    async fn existing_tag_returns_its_id() {
        let db = TestDb::with_sql(SEED).await;
        let first = get(&db, "rust").await.expect("новый тег должен вернуть id");
        let second = get(&db, "rust").await.expect("уже существующий тег тоже должен вернуть id, а пришло ничего: DO NOTHING при конфликте не возвращает строк");
        assert_eq!(first, second, "оба раза должен вернуться один и тот же id");
        assert_eq!(db.table("SELECT id FROM tags").await.len(), 1, "тег должен остаться один");
    }

    #[tokio::test]
    async fn parallel_calls_agree() {
        let db = TestDb::with_sql(SEED).await;
        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move {
                sqlx::query_scalar::<_, i64>(tag_id()).bind("sql").fetch_optional(&pool).await
            }));
        }
        let mut ids = Vec::new();
        for h in handles {
            let id = h.await.expect("задача упала").unwrap_or_else(|e| panic!("запрос не выполнился: {e}"));
            ids.push(id.expect("каждый вызов обязан получить id"));
        }
        assert!(ids.windows(2).all(|w| w[0] == w[1]), "все параллельные вызовы должны получить один id: {ids:?}");
        assert_eq!(db.table("SELECT id FROM tags").await.len(), 1, "тег должен остаться один");
    }
}
