// `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` не только строит план, но и выполняет запрос и приносит
// факты: "Actual Rows" (сколько строк узел вернул на самом деле) и "Shared Hit Blocks" / "Shared Read
// Blocks" (сколько страниц взято из кэша и с диска). Ошибка оценки (план ожидал одно, а пришло другое)
// это главный источник плохих планов.
// TODO: `misestimates` возвращает типы узлов всего дерева (не только корня), у которых "Plan Rows" и
// "Actual Rows" отличаются в 10 раз и больше в любую сторону (нули считай единицами).
// `pages_touched` возвращает сумму "Shared Hit Blocks" и "Shared Read Blocks" корневого узла.

use serde_json::Value;

fn misestimates(plan_json: &str) -> Vec<String> {
    let _ = plan_json;
    Vec::new()
}

fn pages_touched(plan_json: &str) -> i64 {
    let _ = plan_json;
    0
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const TABLE: &str = "
        CREATE TABLE ev (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, country text NOT NULL, city text NOT NULL);
        INSERT INTO ev (country, city) SELECT 'c' || (g % 20), 'city' || (g % 20) FROM generate_series(1, 100000) g;
        ANALYZE ev;
    ";

    async fn analyzed(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}")).await.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn accurate_estimate_is_not_reported() {
        let db = TestDb::with_sql(TABLE).await;
        let text = analyzed(&db, "SELECT id FROM ev WHERE country = 'c3'").await;
        assert_eq!(misestimates(&text), Vec::<String>::new(), "по одному столбцу статистика точна (5000 строк против 5000), ошибок оценки нет:\n{text}");
    }

    #[tokio::test]
    async fn correlated_columns_fool_the_planner() {
        let db = TestDb::with_sql(TABLE).await;
        let text = analyzed(&db, "SELECT id FROM ev WHERE country = 'c3' AND city = 'city3'").await;
        assert_eq!(misestimates(&text), vec!["Seq Scan".to_string()], "country и city всегда идут вместе, но планировщик умножает вероятности (ожидает 250 строк вместо 5000):\n{text}");
        let text = analyzed(&db, "SELECT count(*) FROM ev WHERE country = 'c3' AND city = 'city3'").await;
        assert_eq!(misestimates(&text), vec!["Seq Scan".to_string()], "ошибка сидит в дочернем узле под Aggregate, обходить надо всё дерево:\n{text}");
    }

    #[tokio::test]
    async fn pages_include_everything_a_scan_reads() {
        let db = TestDb::with_sql(TABLE).await;
        let text = analyzed(&db, "SELECT count(*) FROM ev").await;
        let pages: i64 = db.table("SELECT relpages::bigint FROM pg_class WHERE oid = 'ev'::regclass").await.cell(0, 0).parse().unwrap();
        let touched = pages_touched(&text);
        assert!(touched >= pages, "полный перебор таблицы из {pages} страниц должен коснуться не меньше страниц, а функция вернула {touched}:\n{text}");
        assert!(touched < pages * 3, "страниц получилось подозрительно много ({touched}): считай только корневой узел, дочерние уже включены в него");
    }
}
