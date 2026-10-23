fn index_sql() -> &'static str {
    "CREATE INDEX posts_author_idx ON posts (author_id)"
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

    const QUERY: &str = "SELECT p.id, p.body FROM follows f JOIN posts p ON p.author_id = f.followee_id WHERE f.follower_id = 77";

    #[tokio::test]
    async fn baseline_reads_the_whole_table() {
        let db = TestDb::with_sql(SEED).await;
        let p = plan(&db, QUERY).await;
        assert!(seq_scan_on(&p, "posts"), "без индекса под внешний ключ ожидалось чтение всех постов: {}", summary(&p));
        assert!(has_node(&p, "Hash Join"), "без индекса ожидался Hash Join: {}", summary(&p));
    }

    #[tokio::test]
    async fn nested_loop_with_index_probes() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, QUERY).await;
        assert!(has_node(&p, "Nested Loop"), "ожидался Nested Loop: по каждой подписке поиск постов по индексу. План: {}", summary(&p));
        assert!(!seq_scan_on(&p, "posts"), "посты не должны читаться целиком: {}", summary(&p));
    }
}
