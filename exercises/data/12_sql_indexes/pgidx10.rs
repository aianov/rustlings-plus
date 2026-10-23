// Пять процентов постов удалены мягко (`deleted_at` заполнен), а лента показывает только живые:
// `WHERE author_id = ? AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 20`. Обычный индекс
// хранит и удалённые посты, хотя ни один запрос их не ищет.
// TODO: Верни команду создания индекса, который содержит только живые посты и отдаёт ленту автора
// в нужном порядке без сортировки.

fn index_sql() -> &'static str {
    "CREATE INDEX posts_feed_idx ON posts (author_id, created_at DESC)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");
    use serde_json::Value;

    async fn plan(db: &TestDb, sql: &str) -> Value {
        let t = db.table(&format!("EXPLAIN (FORMAT JSON) {sql}")).await;
        let v: Value = serde_json::from_str(t.cell(0, 0)).expect("EXPLAIN (FORMAT JSON) вернул не JSON");
        v[0]["Plan"].clone()
    }

    fn nodes(root: &Value) -> Vec<&Value> {
        fn walk<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
            out.push(node);
            if let Some(children) = node["Plans"].as_array() {
                for child in children {
                    walk(child, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out
    }

    fn summary(root: &Value) -> String {
        nodes(root)
            .iter()
            .map(|n| {
                let kind = n["Node Type"].as_str().unwrap_or("?");
                match n["Relation Name"].as_str() {
                    Some(rel) => format!("{kind} on {rel}"),
                    None => kind.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn index_names(root: &Value) -> Vec<String> {
        nodes(root).iter().filter_map(|n| n["Index Name"].as_str().map(String::from)).collect()
    }

    fn has_node(root: &Value, kind: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.contains(kind)))
    }

    const FEED: &str = "SELECT id, body FROM posts WHERE author_id = 1 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 20";

    #[tokio::test]
    async fn feed_uses_a_partial_index() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, FEED).await;
        assert!(!has_node(&p, "Sort"), "лента всё ещё сортируется: {}", summary(&p));
        let used = index_names(&p);
        assert!(!used.is_empty(), "лента должна идти по индексу: {}", summary(&p));
        let partial: bool = sqlx::query_scalar("SELECT indpred IS NOT NULL FROM pg_index WHERE indexrelid = $1::regclass")
            .bind(&used[0])
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert!(partial, "индекс {} содержит и удалённые посты: нужно условие WHERE deleted_at IS NULL у самого индекса", used[0]);
    }

    #[tokio::test]
    async fn deleted_posts_stay_out_of_the_feed() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("INSERT INTO posts (author_id, body, created_at, deleted_at) VALUES (1, 'удалённый', now() + interval '5 minutes', now())").await;
        let t = db.table(FEED).await;
        assert!(!t.col("body").contains(&"удалённый"), "удалённый пост не должен попадать в ленту:\n{t}");
    }
}
