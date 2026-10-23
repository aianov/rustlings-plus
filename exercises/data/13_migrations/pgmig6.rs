// В таблице постов двести тысяч строк, а столбец `lang` придумали nullable, хотя пустых значений в нём давно
// нет. Нужно сделать его `NOT NULL`. Простой `ALTER ... SET NOT NULL` берёт самую жёсткую блокировку
// и держит её, пока проверяет все строки: пока он читает таблицу, не пишет и не читает никто.
// На большой таблице это минуты простоя. Есть путь, где жёсткая блокировка берётся на мгновение, а
// долгая проверка идёт без остановки записи.
// TODO: `statements` возвращает команды по очереди. В конце `lang` обязан быть NOT NULL, а временных
// ограничений быть не должно.

fn statements() -> Vec<&'static str> {
    vec!["ALTER TABLE posts ALTER COLUMN lang SET NOT NULL"]
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SETUP: &str = "
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint NOT NULL, lang text);
        INSERT INTO posts (author_id, lang) SELECT g % 100, CASE WHEN g % 3 = 0 THEN 'ru' ELSE 'en' END FROM generate_series(1, 200000) g;
    ";

    async fn count(db: &TestDb, sql: &str) -> i64 {
        db.table(sql).await.cell(0, 0).parse().unwrap()
    }

    #[tokio::test]
    async fn column_becomes_not_null_and_checks_are_gone() {
        let db = TestDb::with_sql(SETUP).await;
        for stmt in statements() {
            db.run(stmt).await;
        }
        let t = db.table("SELECT is_nullable FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'lang'").await;
        assert_eq!(t.cell(0, 0), "NO", "после миграции столбец lang должен быть NOT NULL");
        assert_eq!(count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'c'").await, 0, "временное ограничение CHECK должно быть удалено");
        let res = db.fails("INSERT INTO posts (author_id, lang) VALUES (1, NULL)").await;
        assert_eq!(res.map(|e| e.code), Some("23502".to_string()), "вставка NULL в lang должна быть отвергнута (NOT NULL)");
    }

    #[tokio::test]
    async fn the_long_check_runs_before_the_hard_step() {
        let db = TestDb::with_sql(SETUP).await;
        let mut saw_not_valid = false;
        for stmt in statements() {
            let low = stmt.to_lowercase();
            if low.contains("set not null") {
                let validated = count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'c' AND convalidated").await;
                assert!(validated >= 1, "перед SET NOT NULL нужно ограничение CHECK (lang IS NOT NULL), уже проверенное по всей таблице: иначе SET NOT NULL сам читает двести тысяч строк под жёсткой блокировкой");
            }
            db.run(stmt).await;
            if count(&db, "SELECT count(*) FROM pg_constraint WHERE conrelid = 'posts'::regclass AND contype = 'c' AND NOT convalidated").await > 0 {
                saw_not_valid = true;
            }
        }
        assert!(saw_not_valid, "ограничение должно сначала появиться непроверенным (NOT VALID), чтобы жёсткая блокировка была краткой, а проверка существующих строк шла отдельной командой");
    }
}
