// Вход в соцсеть: ищем пользователя по почте без учёта регистра, `WHERE lower(email) = ...`.
// Индекс по `email` есть, но поиск при входе всё равно читает пять тысяч пользователей подряд.
// А у соседнего сервиса таблица на миллионы.
// TODO: Исправь скрипт миграции, чтобы запрос из условия перестал читать таблицу целиком.
// Лишних индексов по почте после миграции быть не должно.

fn index_sql() -> &'static str {
    "CREATE INDEX users_email_idx ON users (email)"
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

    const QUERY: &str = "SELECT id FROM users WHERE lower(email) = 'user77@example.com'";

    #[tokio::test]
    async fn lookup_uses_the_index() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        db.run("ANALYZE users").await;
        let p = plan(&db, QUERY).await;
        assert!(!seq_scan_on(&p, "users"), "поиск по lower(email) читает таблицу целиком: {}. Индекс по голому email этот запрос не обслуживает", summary(&p));
        let t = db.table(QUERY).await;
        assert_eq!(t.col("id"), vec!["77"], "почта User77@Example.com должна находиться по нижнему регистру");
    }

    #[tokio::test]
    async fn only_one_index_on_email() {
        let db = TestDb::with_sql(SEED).await;
        db.run(index_sql()).await;
        let defs = index_defs(&db, "users").await;
        assert_eq!(defs.len(), 1, "должен остаться один индекс по почте, а их: {defs:?}");
    }
}
