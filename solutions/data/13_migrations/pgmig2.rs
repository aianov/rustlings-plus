use sqlx::PgPool;

async fn migrate(pool: &PgPool, files: &[(i64, &str, &str)]) -> Result<usize, (i64, sqlx::Error)> {
    let done: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations").fetch_all(pool).await.map_err(|e| (0, e))?;
    let mut applied = 0;
    for (version, name, sql) in files {
        if done.contains(version) {
            continue;
        }
        apply_one(pool, *version, name, sql).await.map_err(|e| (*version, e))?;
        applied += 1;
    }
    Ok(applied)
}

async fn apply_one(pool: &PgPool, version: i64, name: &str, sql: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
        .bind(version)
        .bind(name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const LOG: &str = "CREATE TABLE schema_migrations (version bigint PRIMARY KEY, name text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())";

    async fn exists(db: &TestDb, table: &str) -> bool {
        db.table(&format!("SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = '{table}'")).await.cell(0, 0) == "1"
    }

    #[tokio::test]
    async fn failed_migration_leaves_nothing_behind() {
        let db = TestDb::with_sql(LOG).await;
        let files = [(1, "base", "CREATE TABLE a (id int)"), (2, "broken", "CREATE TABLE b (id int); CREATE TABLE a (id int); CREATE TABLE c (id int)")];
        match migrate(&db.pool, &files).await {
            Err((version, _)) => assert_eq!(version, 2, "упасть должна вторая миграция"),
            Ok(n) => panic!("миграция 2 пытается создать a второй раз и должна упасть, а применено {n}"),
        }
        assert!(exists(&db, "a").await, "результат первой миграции должен остаться");
        assert!(!exists(&db, "b").await, "первая команда упавшей миграции должна откатиться, таблица b осталась");
        assert!(!exists(&db, "c").await, "после ошибки следующие команды не выполняются");
        let t = db.table("SELECT version FROM schema_migrations ORDER BY version").await;
        assert_eq!(t.col("version"), vec!["1"], "записана должна быть только версия 1");
    }

    #[tokio::test]
    async fn fixed_file_applies_cleanly_afterwards() {
        let db = TestDb::with_sql(LOG).await;
        let broken = [(1, "base", "CREATE TABLE a (id int)"), (2, "broken", "CREATE TABLE b (id int); CREATE TABLE a (id int)")];
        assert!(migrate(&db.pool, &broken).await.is_err(), "сломанная миграция должна упасть");
        let fixed = [(1, "base", "CREATE TABLE a (id int)"), (2, "fixed", "CREATE TABLE b (id int); CREATE TABLE c (id int)")];
        let n = migrate(&db.pool, &fixed).await.unwrap_or_else(|(v, e)| panic!("после исправления файла миграция {v} должна пройти: {e}"));
        assert_eq!(n, 1, "применяется только исправленная миграция");
        assert!(exists(&db, "b").await && exists(&db, "c").await, "обе таблицы исправленной миграции должны существовать");
    }

    #[tokio::test]
    async fn failed_record_takes_the_sql_with_it() {
        let db = TestDb::with_sql(LOG).await;
        let files = [(1, "first", "CREATE TABLE a (id int)"), (1, "twin", "CREATE TABLE twin (id int)")];
        assert!(migrate(&db.pool, &files).await.is_err(), "вторая запись с той же версией нарушает первичный ключ журнала");
        assert!(!exists(&db, "twin").await, "если запись о версии не удалась, SQL миграции тоже должен откатиться (так выглядит падение процесса между командами и записью)");
    }
}
