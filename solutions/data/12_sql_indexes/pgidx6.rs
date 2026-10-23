fn index_sql() -> &'static str {
    "CREATE INDEX posts_author_time_idx ON posts (author_id, created_at DESC)"
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

    fn has_node(root: &Value, kind: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.contains(kind)))
    }

    const PROLIFIC: &str = "SELECT id, body, created_at FROM posts WHERE author_id = 1 ORDER BY created_at DESC LIMIT 20";
    const ORDINARY: &str = "SELECT id, body, created_at FROM posts WHERE author_id = 123 ORDER BY created_at DESC LIMIT 20";

    #[tokio::test]
    async fn sorting_is_gone_for_the_busy_author() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, PROLIFIC).await;
        assert!(!has_node(&p, "Sort"), "у автора 1 двадцать пять тысяч постов, и база всё ещё сортирует их все: {}. Индекс должен отдавать строки уже в нужном порядке", summary(&p));
        assert!(!seq_scan_on(&p, "posts"), "таблица не должна читаться целиком: {}", summary(&p));
    }

    #[tokio::test]
    async fn ordinary_authors_do_not_scan_the_table() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, ORDINARY).await;
        assert!(!seq_scan_on(&p, "posts"), "для обычного автора тоже нужен индекс: {}", summary(&p));
    }

    #[tokio::test]
    async fn new_posts_appear_first() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("INSERT INTO posts (author_id, body, created_at) VALUES (1, 'самый свежий', now() + interval '1 minute')").await;
        let t = db.table(PROLIFIC).await;
        assert_eq!(t.cell(0, 1), "самый свежий", "после вставки новый пост должен быть первым:\n{t}");
    }
}
