// Профиль автора тормозит. Разработчик сказал: «Я же создал индекс по времени и автору!»
// Индекс есть, а план по-прежнему читает таблицу целиком. Все остальные запросы работают как работали.
// TODO: Исправь скрипт миграции так, чтобы запрос `WHERE author_id = 123 ORDER BY created_at DESC
// LIMIT 20` перестал читать всю таблицу. Лишних индексов после миграции быть не должно.

fn index_sql() -> &'static str {
    "CREATE INDEX posts_time_author_idx ON posts (created_at, author_id)"
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

    fn seq_scan_on(root: &Value, relation: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.ends_with("Seq Scan")) && n["Relation Name"] == relation)
    }

    async fn index_defs(db: &TestDb, table: &str) -> Vec<String> {
        let t = db
            .table(&format!("SELECT indexdef FROM pg_indexes WHERE schemaname = current_schema() AND tablename = '{table}' AND indexname NOT LIKE '%pkey' AND indexname NOT LIKE '%username_key' ORDER BY indexname"))
            .await;
        t.column(0).into_iter().map(String::from).collect()
    }

    const QUERY: &str = "SELECT id, body, created_at FROM posts WHERE author_id = 123 ORDER BY created_at DESC LIMIT 20";

    #[tokio::test]
    async fn author_query_uses_the_index() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, QUERY).await;
        assert!(!seq_scan_on(&p, "posts"), "план по-прежнему читает таблицу целиком: {}. Индекс начинается со столбца, которого нет в условии равенства", summary(&p));
    }

    #[tokio::test]
    async fn no_leftover_indexes() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        let defs = index_defs(&db, "posts").await;
        assert_eq!(defs.len(), 1, "после миграции должен остаться один новый индекс, а их: {defs:?}");
        assert!(defs[0].contains("(author_id"), "индекс должен начинаться со столбца с равенством: {}", defs[0]);
    }
}
