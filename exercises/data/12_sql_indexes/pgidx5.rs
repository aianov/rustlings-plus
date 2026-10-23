// Миграция для постов создаёт два индекса: по автору (для профиля) и по флагу `published`
// (кто-то решил, что «по флагу быстрее фильтровать»). Приложение читает только опубликованные посты
// (99% таблицы), запросов вида «покажи неопубликованные» нет. Профиль работает быстро, но индекс по
// флагу занимает место и тормозит каждую вставку, а пользы от него нет.
// TODO: `migration` возвращает исправленный скрипт: нужное оставь, бесполезное убери.

fn migration() -> &'static str {
    "CREATE INDEX posts_author_idx ON posts (author_id); CREATE INDEX posts_published_idx ON posts (published)"
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

    #[tokio::test]
    async fn only_useful_indexes_remain() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migration()).await;
        db.run("ANALYZE posts").await;
        let defs = index_defs(&db, "posts").await;
        assert!(!defs.iter().any(|d| d.contains("(published)")), "индекс по одному булеву столбцу не нужен: по условию `published` подходит 99% таблицы, и база всё равно читает её подряд. Остались индексы: {defs:?}");
        let p = plan(&db, "SELECT id, body FROM posts WHERE author_id = 123 AND published").await;
        assert!(!seq_scan_on(&p, "posts"), "профиль автора должен остаться быстрым: {}", summary(&p));
    }

    #[tokio::test]
    async fn the_flag_alone_never_uses_an_index() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migration()).await;
        db.run("ANALYZE posts").await;
        let p = plan(&db, "SELECT count(*) FROM posts WHERE published").await;
        assert!(seq_scan_on(&p, "posts"), "для выборки 99% таблицы ожидается чтение подряд, а не индекс: {}", summary(&p));
    }
}
