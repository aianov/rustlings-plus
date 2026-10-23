fn indexes() -> Vec<&'static str> {
    vec![
        "CREATE INDEX posts_profile_idx ON posts (author_id, created_at DESC, id DESC) WHERE deleted_at IS NULL",
        "CREATE INDEX follows_followers_idx ON follows (followee_id, created_at DESC, follower_id DESC)",
        "CREATE INDEX users_username_lower_idx ON users (lower(username))",
    ]
}

fn profile_first_sql() -> &'static str {
    "SELECT id, body, created_at FROM posts WHERE author_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC, id DESC LIMIT 20"
}

fn profile_next_sql() -> &'static str {
    "SELECT id, body, created_at FROM posts WHERE author_id = $1 AND deleted_at IS NULL AND (created_at, id) < ($2, $3) ORDER BY created_at DESC, id DESC LIMIT 20"
}

fn followers_first_sql() -> &'static str {
    "SELECT f.follower_id, u.username, f.created_at FROM follows f JOIN users u ON u.id = f.follower_id WHERE f.followee_id = $1 ORDER BY f.created_at DESC, f.follower_id DESC LIMIT 5"
}

fn followers_next_sql() -> &'static str {
    "SELECT f.follower_id, u.username, f.created_at FROM follows f JOIN users u ON u.id = f.follower_id WHERE f.followee_id = $1 AND (f.created_at, f.follower_id) < ($2, $3) ORDER BY f.created_at DESC, f.follower_id DESC LIMIT 5"
}

fn login_sql() -> &'static str {
    "SELECT id FROM users WHERE lower(username) = lower($1)"
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

    use chrono::{DateTime, Utc};

    type Row = (i64, String, DateTime<Utc>);

    fn seq_scan_on_any(root: &Value) -> bool {
        nodes(root).iter().any(|n| n["Node Type"].as_str().is_some_and(|t| t.ends_with("Seq Scan")))
    }

    async fn prepared() -> TestDb {
        let db = TestDb::with_sql(SEED).await;
        assert!(indexes().len() <= 4, "новых индексов должно быть не больше четырёх, а их {}", indexes().len());
        for stmt in indexes() {
            db.run(stmt).await;
        }
        for table in ["posts", "follows", "users"] {
            db.run(&format!("VACUUM ANALYZE {table}")).await;
        }
        db
    }

    fn no_offset(sql: &str) {
        assert!(!sql.to_lowercase().contains("offset"), "OFFSET в запросе страницы нельзя: он читает и выбрасывает пропущенные строки. Запрос: {sql}");
    }

    async fn walk(db: &TestDb, first: &str, next: &str, key: i64, pages: usize) -> Vec<i64> {
        no_offset(first);
        no_offset(next);
        let mut rows: Vec<Row> = sqlx::query_as(first).bind(key).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("первая страница не выполнилась: {e}"));
        let mut ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        for _ in 1..pages {
            let Some(last) = rows.last().cloned() else { break };
            rows = sqlx::query_as(next)
                .bind(key)
                .bind(last.2)
                .bind(last.0)
                .fetch_all(&db.pool)
                .await
                .unwrap_or_else(|e| panic!("следующая страница не выполнилась: {e}. Параметры: $1 ключ, $2 время курсора, $3 id курсора"));
            ids.extend(rows.iter().map(|r| r.0));
        }
        ids
    }

    fn numbers(t: &course_support::pg::Table) -> Vec<i64> {
        t.column(0).iter().map(|s| s.parse().unwrap()).collect()
    }

    #[tokio::test]
    async fn profile_pages_follow_the_cursor() {
        let db = prepared().await;
        let got = walk(&db, profile_first_sql(), profile_next_sql(), 1, 3).await;
        let expected = db.table("SELECT id FROM posts WHERE author_id = 1 AND deleted_at IS NULL ORDER BY created_at DESC, id DESC LIMIT 60").await;
        assert_eq!(got, numbers(&expected), "три страницы профиля автора 1 должны дать его 60 свежих живых постов подряд");
    }

    #[tokio::test]
    async fn profile_plans_are_cheap() {
        let db = prepared().await;
        let first = inline(profile_first_sql(), &["1"]);
        let p = analyzed(&db, &first).await;
        assert!(!has_node(&p, "Sort"), "первая страница профиля сортирует: {}", summary(&p));
        assert!(!seq_scan_on_any(&p), "первая страница читает таблицу целиком: {}", summary(&p));
        let deep = db.table("SELECT created_at::text, id::text FROM posts WHERE author_id = 1 AND deleted_at IS NULL ORDER BY created_at DESC, id DESC OFFSET 15000 LIMIT 1").await;
        let ts = format!("'{}'::timestamptz", deep.cell(0, 0));
        let next = inline(profile_next_sql(), &["1", ts.as_str(), deep.cell(0, 1)]);
        let p = analyzed(&db, &next).await;
        assert!(!has_node(&p, "Sort"), "глубокая страница профиля сортирует: {}", summary(&p));
        assert!(removed_by_filter(&p) <= 5, "глубокая страница прочитала и выбросила {} строк: курсор должен быть условием индекса. План: {}", removed_by_filter(&p), summary(&p));
    }

    #[tokio::test]
    async fn followers_pages_follow_the_cursor() {
        let db = prepared().await;
        let got = walk(&db, followers_first_sql(), followers_next_sql(), 123, 3).await;
        let expected = db.table("SELECT follower_id FROM follows WHERE followee_id = 123 ORDER BY created_at DESC, follower_id DESC LIMIT 15").await;
        assert_eq!(got, numbers(&expected), "три страницы по пять подписчиков пользователя 123 должны дать всех его подписчиков без пропусков и повторов");
        let p = analyzed(&db, &inline(followers_first_sql(), &["123"])).await;
        assert!(!has_node(&p, "Sort"), "список подписчиков сортируется: {}", summary(&p));
        assert!(!seq_scan_on_any(&p), "список подписчиков читает таблицу целиком: {}", summary(&p));
    }

    #[tokio::test]
    async fn login_uses_the_index() {
        let db = prepared().await;
        let p = analyzed(&db, &inline(login_sql(), &["'USER77'"])).await;
        assert!(!seq_scan_on_any(&p), "вход по нику читает таблицу целиком: {}", summary(&p));
        let id = sqlx::query_scalar::<_, i64>(login_sql()).bind("USER77").fetch_one(&db.pool).await.expect("запрос входа");
        assert_eq!(id, 77, "ник USER77 должен находиться без учёта регистра");
    }
}
