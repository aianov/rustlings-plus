fn index_sql() -> &'static str {
    "CREATE INDEX posts_feed_idx ON posts (created_at DESC, id DESC)"
}

fn page_sql() -> &'static str {
    "SELECT id, created_at FROM posts WHERE (created_at, id) < ($1, $2) ORDER BY created_at DESC, id DESC LIMIT 20"
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

    fn removed_by_filter(root: &Value) -> i64 {
        nodes(root).iter().map(|n| n["Rows Removed by Filter"].as_i64().unwrap_or(0)).sum()
    }

    fn has_node(root: &Value, kind: &str) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.contains(kind)))
    }

    fn inline(sql: &str, args: &[&str]) -> String {
        let mut out = sql.to_string();
        for (i, arg) in args.iter().enumerate().rev() {
            out = out.replace(&format!("${}", i + 1), arg);
        }
        out
    }

    use chrono::{DateTime, Duration, Utc};

    #[tokio::test]
    async fn pages_follow_the_cursor() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        let mut cursor: (DateTime<Utc>, i64) = (Utc::now() + Duration::days(365), i64::MAX);
        let mut ids = Vec::new();
        for page in 0..3 {
            let rows: Vec<(i64, DateTime<Utc>)> = sqlx::query_as(page_sql())
                .bind(cursor.0)
                .bind(cursor.1)
                .fetch_all(&db.pool)
                .await
                .unwrap_or_else(|e| panic!("страница {page} не выполнилась: {e}. Запрос должен принимать $1 (время) и $2 (id) и возвращать id и created_at"));
            assert_eq!(rows.len(), 20, "на странице {page} должно быть двадцать постов");
            ids.extend(rows.iter().map(|r| r.0));
            let last = rows.last().unwrap();
            cursor = (last.1, last.0);
        }
        let expected = db.table("SELECT id FROM posts ORDER BY created_at DESC, id DESC LIMIT 60").await;
        let expected: Vec<i64> = expected.column(0).iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(ids, expected, "три страницы подряд должны дать первые 60 постов ленты без пропусков и повторов");
    }

    #[tokio::test]
    async fn deep_page_costs_as_much_as_the_first() {
        let db = TestDb::with_sql(SEED).await;
        assert!(!page_sql().to_lowercase().contains("offset"), "OFFSET нельзя: он читает и выбрасывает пропущенные строки");
        db.run(index_sql()).await;
        db.run("ANALYZE posts").await;
        let deep = db.table("SELECT created_at::text, id::text FROM posts ORDER BY created_at DESC, id DESC OFFSET 50000 LIMIT 1").await;
        let ts = format!("'{}'::timestamptz", deep.cell(0, 0));
        let sql = inline(page_sql(), &[ts.as_str(), deep.cell(0, 1)]);
        let p = analyzed(&db, &sql).await;
        assert!(!has_node(&p, "Sort"), "страница сортируется: {}", summary(&p));
        assert!(removed_by_filter(&p) <= 5, "база прочитала и выбросила {} строк: условие курсора должно стать условием индекса, а не фильтром. План: {}", removed_by_filter(&p), summary(&p));
        let scan = nodes(&p).into_iter().find(|n| n["Node Type"].as_str().is_some_and(|t| t.contains("Index"))).expect("в плане должен быть узел индекса");
        assert!(scan["Actual Rows"].as_i64().unwrap_or(0) <= 20, "индекс должен отдать ровно страницу, а не всё, что старше курсора");
    }
}
