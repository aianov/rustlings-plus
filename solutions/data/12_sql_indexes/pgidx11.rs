fn statements() -> Vec<&'static str> {
    vec![
        "CREATE INDEX follows_followee_idx ON follows (followee_id, created_at DESC) INCLUDE (follower_id)",
        "VACUUM follows",
    ]
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");
    use serde_json::Value;

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

    async fn analyzed(db: &TestDb, sql: &str) -> Value {
        let t = db.table(&format!("EXPLAIN (ANALYZE, FORMAT JSON) {sql}")).await;
        let v: Value = serde_json::from_str(t.cell(0, 0)).expect("EXPLAIN (ANALYZE, FORMAT JSON) вернул не JSON");
        v[0]["Plan"].clone()
    }

    fn has_node(root: &Value, kind: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.contains(kind)))
    }

    const QUERY: &str = "SELECT follower_id, created_at FROM follows WHERE followee_id = 123 ORDER BY created_at DESC LIMIT 50";

    #[tokio::test]
    async fn answered_from_the_index_alone() {
        let db = TestDb::with_sql(SEED).await;
        let list = statements();
        for stmt in &list {
            db.run(stmt).await;
        }
        let mut p = analyzed(&db, QUERY).await;
        for _ in 0..6 {
            let ready = nodes(&p).iter().any(|n| n["Node Type"] == "Index Only Scan" && n["Heap Fetches"].as_i64() == Some(0));
            if ready {
                break;
            }
            for stmt in list.iter().filter(|s| s.to_lowercase().contains("vacuum")) {
                db.run(stmt).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            p = analyzed(&db, QUERY).await;
        }
        assert!(has_node(&p, "Index Only Scan"), "ожидался Index Only Scan, а в плане: {}. Индекс должен содержать все нужные запросу столбцы, а карта видимости быть заполнена", summary(&p));
        let node = nodes(&p).into_iter().find(|n| n["Node Type"] == "Index Only Scan").unwrap();
        let fetches = node["Heap Fetches"].as_i64().unwrap_or(-1);
        assert_eq!(fetches, 0, "Index Only Scan всё равно сходил в таблицу {fetches} раз: база не знает, что страницы целиком видимы всем. Помогает VACUUM");
    }

    #[tokio::test]
    async fn rows_are_correct() {
        let db = TestDb::with_sql(SEED).await;
        for stmt in statements() {
            db.run(stmt).await;
        }
        let t = db.table(QUERY).await;
        assert_eq!(t.len(), 12, "у пользователя 123 двенадцать подписчиков:\n{t}");
    }
}
