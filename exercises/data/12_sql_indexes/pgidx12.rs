// Общая лента: двадцать последних постов во всей сети, `ORDER BY created_at DESC, id DESC LIMIT 20`.
// Второй ключ нужен, чтобы порядок был однозначным при равном времени. Индекс только по
// `created_at` почти решает проблему, но база всё равно досортировывает что-то по `id`.
// TODO: Верни команду создания индекса, после которого в плане нет ни одной сортировки.

fn index_sql() -> &'static str {
    "CREATE INDEX posts_created_idx ON posts (created_at DESC)"
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

    fn has_node(root: &Value, kind: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.contains(kind)))
    }

    fn index_names(root: &Value) -> Vec<String> {
        nodes(root).iter().filter_map(|n| n["Index Name"].as_str().map(String::from)).collect()
    }

    const QUERY: &str = "SELECT id, author_id, body FROM posts ORDER BY created_at DESC, id DESC LIMIT 20";

    #[tokio::test]
    async fn no_sorting_at_all() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, QUERY).await;
        assert!(!has_node(&p, "Sort"), "в плане осталась сортировка: {}. Порядок индекса должен совпадать с ORDER BY целиком, включая второй ключ", summary(&p));
        assert!(!index_names(&p).is_empty(), "лента должна идти по индексу: {}", summary(&p));
    }

    #[tokio::test]
    async fn order_is_unambiguous() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("INSERT INTO posts (author_id, body, created_at) VALUES (1, 'a', timestamptz '2099-01-01 00:00:00+00'), (1, 'b', timestamptz '2099-01-01 00:00:00+00')").await;
        let t = db.table("SELECT body FROM posts ORDER BY created_at DESC, id DESC LIMIT 2").await;
        assert_eq!(t.col("body"), vec!["b", "a"], "при равном времени больший id идёт первым");
    }
}
