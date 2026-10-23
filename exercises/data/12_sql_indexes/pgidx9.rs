// Отчёт «сколько постов опубликовано три дня назад» считает посты по дате. Индекс по `created_at`
// есть, а запрос читает всю таблицу. В посте, созданном ровно в полночь, граница дня должна
// попадать в тот день, который начинается в эту полночь, а не в предыдущий.
// TODO: Перепиши запрос так, чтобы индекс заработал, а числа остались прежними.

fn count_sql() -> &'static str {
    "SELECT count(*) FROM posts WHERE created_at::date = current_date - 3"
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

    async fn prepared() -> TestDb {
        let db = TestDb::with_sql(SEED).await;
        db.run("CREATE INDEX posts_created_idx ON posts (created_at)").await;
        db.run("INSERT INTO posts (author_id, body, created_at) VALUES (2, 'начало дня', (current_date - 3)::timestamptz), (2, 'начало следующего дня', (current_date - 2)::timestamptz)").await;
        db.run("ANALYZE posts").await;
        db
    }

    #[tokio::test]
    async fn same_numbers_as_the_date_cast() {
        let db = prepared().await;
        let expected = db.table("SELECT count(*) FROM posts WHERE created_at::date = current_date - 3").await;
        let got = db.table(count_sql()).await;
        assert_eq!(got.cell(0, 0), expected.cell(0, 0), "число постов за день должно совпасть с исходным запросом (пост ровно в полночь начала дня считается, пост ровно в полночь конца дня нет)");
    }

    #[tokio::test]
    async fn index_works_now() {
        let db = prepared().await;
        let p = plan(&db, count_sql()).await;
        assert!(!seq_scan_on(&p, "posts"), "запрос читает таблицу целиком: {}. Условие над столбцом (`created_at::date`) не использует индекс по самому столбцу", summary(&p));
    }
}
