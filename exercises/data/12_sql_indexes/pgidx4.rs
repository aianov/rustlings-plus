// Ночной импорт грузит сто тысяч строк в таблицу `imports`, и сразу после него сервис ищет там
// `source = 'legacy'`: так помечены все загруженные строки. Планировщик почему-то строит план
// под выборку из нескольких сотен строк (через индекс) и читает таблицу тормознее, чем если бы просто
// прошёл её подряд. Разработчик решил, что дело в раздутой таблице, и добавил после загрузки команду.
// Помогает не очень.
// TODO: `after_load` возвращает команду, которую надо выполнить после загрузки, чтобы план соответствовал
// реальным данным. Найди, что на самом деле нужно базе.

fn after_load() -> &'static str {
    "VACUUM imports"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use serde_json::Value;

    const SETUP: &str = "
        CREATE TABLE imports (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, source text NOT NULL, payload text NOT NULL);
        CREATE INDEX imports_source_idx ON imports (source);
        ANALYZE imports;
    ";

    #[tokio::test]
    async fn plan_follows_real_data_after_the_load() {
        let db = TestDb::with_sql(SETUP).await;
        db.run("INSERT INTO imports (source, payload) SELECT 'legacy', 'row ' || g FROM generate_series(1, 100000) g").await;
        db.run(after_load()).await;
        let t = db.table("EXPLAIN (FORMAT JSON) SELECT id FROM imports WHERE source = 'legacy'").await;
        let v: Value = serde_json::from_str(t.cell(0, 0)).expect("план в JSON");
        let root = &v[0]["Plan"];
        let rows = root["Plan Rows"].as_f64().unwrap_or(0.0);
        assert!(rows >= 50_000.0, "под условие подходят все 100000 строк, а планировщик ожидает {rows}: статистика таблицы осталась от пустой таблицы");
        assert_eq!(root["Node Type"], "Seq Scan", "когда нужна вся таблица, дешевле всего читать её подряд, а не прыгать по индексу");
    }

    #[tokio::test]
    async fn the_index_stays() {
        let db = TestDb::with_sql(SETUP).await;
        db.run(after_load()).await;
        let t = db.table("SELECT count(*) FROM pg_indexes WHERE schemaname = current_schema() AND indexname = 'imports_source_idx'").await;
        assert_eq!(t.cell(0, 0), "1", "индекс удалять нельзя: проблема не в нём, а в том, что о таблице знает планировщик");
    }
}
