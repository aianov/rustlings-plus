// `CREATE UNIQUE INDEX CONCURRENTLY` построил индекс почти до конца и упал: нашёл дубликаты. В отличие от
// обычного CREATE INDEX, он не откатывается бесследно, а оставляет в базе индекс-обломок с пометкой
// «непригоден». Индекс в запросах не участвует, но на каждую запись продолжает обновляться и, что хуже,
// не защищает от дубликатов. Повторный CREATE с тем же именем отвечает «already exists».
// TODO: `repair_invalid_indexes` находит все непригодные индексы текущей схемы (`pg_index.indisvalid`),
// удаляет каждый и создаёт заново с тем же определением, без блокировки записи. Возвращает имена
// исправленных. Исправных индексов она касаться не должна.

use sqlx::PgPool;

async fn repair_invalid_indexes(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let _ = pool;
    Ok(Vec::new())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SETUP: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, email text NOT NULL, city text);
        INSERT INTO users (email, city) VALUES ('a@x', 'Омск'), ('a@x', 'Томск'), ('b@x', 'Омск');
        CREATE INDEX users_city_idx ON users (city);
    ";

    async fn broken_db() -> TestDb {
        let db = TestDb::with_sql(SETUP).await;
        let err = db.fails("CREATE UNIQUE INDEX CONCURRENTLY users_email_uniq ON users (email)").await;
        assert_eq!(err.map(|e| e.code), Some("23505".to_string()), "подготовка: индекс должен упасть на дубликатах");
        db
    }

    async fn state(db: &TestDb, name: &str) -> (String, String) {
        let t = db.table(&format!("SELECT i.indisvalid::text, c.oid::text FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid WHERE c.relname = '{name}' AND c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = current_schema())")).await;
        (t.cell(0, 0).to_string(), t.cell(0, 1).to_string())
    }

    #[tokio::test]
    async fn broken_index_is_rebuilt() {
        let db = broken_db().await;
        assert_eq!(state(&db, "users_email_uniq").await.0, "false", "подготовка: индекс-обломок должен быть непригоден");
        db.run("DELETE FROM users WHERE id = 2").await;
        let fixed = repair_invalid_indexes(&db.pool).await.expect("ремонт индексов");
        assert_eq!(fixed, vec!["users_email_uniq".to_string()], "исправлен должен быть один индекс");
        assert_eq!(state(&db, "users_email_uniq").await.0, "true", "после ремонта индекс должен быть пригоден");
        let dup = db.fails("INSERT INTO users (email) VALUES ('a@x')").await;
        assert_eq!(dup.map(|e| e.code), Some("23505".to_string()), "исправленный индекс уникальный и должен отвергать дубликаты");
    }

    #[tokio::test]
    async fn healthy_indexes_are_untouched() {
        let db = broken_db().await;
        let before = state(&db, "users_city_idx").await;
        db.run("DELETE FROM users WHERE id = 2").await;
        repair_invalid_indexes(&db.pool).await.expect("ремонт индексов");
        assert_eq!(state(&db, "users_city_idx").await, before, "исправный индекс не нужно пересоздавать: его oid должен остаться прежним");
        let again = repair_invalid_indexes(&db.pool).await.expect("повторный ремонт");
        assert!(again.is_empty(), "после ремонта чинить нечего, а вернулось {again:?}");
    }
}
