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

    fn index_names(root: &Value) -> Vec<String> {
        nodes(root).iter().filter_map(|n| n["Index Name"].as_str().map(String::from)).collect()
    }

    const QUERY: &str = "SELECT id, body FROM posts WHERE author_id = 123";

    #[tokio::test]
    async fn baseline_is_a_full_scan() {
        let db = TestDb::with_sql(SEED).await;
        let p = plan(&db, QUERY).await;
        assert!(seq_scan_on(&p, "posts"), "без индекса на author_id ожидался полный перебор, а в плане: {}", summary(&p));
    }

    #[tokio::test]
    async fn index_removes_the_full_scan() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, QUERY).await;
        assert!(!seq_scan_on(&p, "posts"), "после создания индекса таблица всё равно читается целиком: {}. Индекс должен быть по столбцу из WHERE", summary(&p));
        assert!(!index_names(&p).is_empty(), "в плане должен быть узел с индексом: {}", summary(&p));
        let t = db.table(QUERY).await;
        assert!(t.len() > 5, "запрос должен вернуть посты автора 123, вернул {}", t.len());
    }
}
