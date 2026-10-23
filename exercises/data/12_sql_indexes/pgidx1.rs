// Чтобы управлять планами из кода, надо уметь их читать. `EXPLAIN (FORMAT JSON)` возвращает дерево
// в виде текста JSON: массив из одного объекта, внутри ключ "Plan", а у каждого узла есть "Node Type"
// (как база читает данные), "Relation Name" (таблица, если есть) и "Plan Rows" (сколько строк она
// ожидает). Дочерние узлы лежат в "Plans".
// TODO: Допиши `root_summary`: тип корневого узла, имя его таблицы (или "-", если у узла таблицы
// нет) и ожидаемое число строк.

use serde_json::Value;

fn root_summary(plan_json: &str) -> (String, String, f64) {
    let _ = plan_json;
    ("?".to_string(), "?".to_string(), 0.0)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const TABLE: &str = "
        CREATE TABLE posts AS SELECT g AS id, 1 + g % 500 AS author_id FROM generate_series(1, 20000) g;
        ANALYZE posts;
    ";

    async fn json_plan(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN (FORMAT JSON) {sql}")).await.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn reads_a_scan_node() {
        let db = TestDb::with_sql(TABLE).await;
        let text = json_plan(&db, "SELECT id FROM posts WHERE author_id = 7").await;
        let (node, relation, rows) = root_summary(&text);
        assert_eq!(node, "Seq Scan", "индексов нет, база читает таблицу целиком; корневой узел плана:\n{text}");
        assert_eq!(relation, "posts", "у Seq Scan есть таблица");
        assert!((20.0..=80.0).contains(&rows), "у автора 7 около 40 постов из 20000, а в плане ожидается {rows}");
    }

    #[tokio::test]
    async fn reads_nodes_without_a_table() {
        let db = TestDb::with_sql(TABLE).await;
        let text = json_plan(&db, "SELECT count(*) FROM posts").await;
        let (node, relation, rows) = root_summary(&text);
        assert_eq!((node.as_str(), relation.as_str(), rows), ("Aggregate", "-", 1.0), "count(*) без группировки это узел Aggregate без таблицы, одна строка:\n{text}");
        let text = json_plan(&db, "SELECT id FROM posts ORDER BY id LIMIT 5").await;
        let (node, _, rows) = root_summary(&text);
        assert_eq!((node.as_str(), rows), ("Limit", 5.0), "корень плана с LIMIT 5 это узел Limit на пять строк:\n{text}");
    }
}
