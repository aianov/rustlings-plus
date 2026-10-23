use serde_json::Value;

fn seq_scans(plan: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if plan["Node Type"].as_str().is_some_and(|t| t.ends_with("Seq Scan"))
        && let Some(rel) = plan["Relation Name"].as_str()
    {
        out.push(rel.to_string());
    }
    if let Some(children) = plan["Plans"].as_array() {
        for child in children {
            out.extend(seq_scans(child));
        }
    }
    out
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

    #[tokio::test]
    async fn finds_scans_at_any_depth() {
        let db = TestDb::with_sql(SEED).await;
        let p = plan(&db, "SELECT p.id FROM posts p JOIN users u ON u.id = p.author_id WHERE p.body = 'post number 5'").await;
        let mut found = seq_scans(&p);
        found.sort();
        assert_eq!(found, vec!["posts".to_string()], "тело поста не проиндексировано, и Seq Scan по posts спрятан под соединением; users читаются по первичному ключу");
        let p = plan(&db, "SELECT count(*) FROM posts p JOIN users u ON u.id = p.author_id").await;
        let mut found = seq_scans(&p);
        found.sort();
        assert_eq!(found, vec!["posts".to_string(), "users".to_string()], "соединение всей таблицы с таблицей читает обе подряд");
    }

    #[tokio::test]
    async fn empty_after_the_index_helps() {
        let db = TestDb::with_sql(SEED).await;
        let before = plan(&db, "SELECT id FROM posts WHERE author_id = 123").await;
        assert_eq!(seq_scans(&before), vec!["posts".to_string()], "без индекса таблица читается подряд");
        db.run("CREATE INDEX posts_author_idx ON posts (author_id)").await;
        db.run("ANALYZE posts").await;
        let after = plan(&db, "SELECT id FROM posts WHERE author_id = 123").await;
        assert!(seq_scans(&after).is_empty(), "после индекса Seq Scan быть не должно, найдено: {:?}", seq_scans(&after));
    }
}
