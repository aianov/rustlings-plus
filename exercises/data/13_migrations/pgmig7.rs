// Индекс на большой живой таблице создают командой `CREATE INDEX CONCURRENTLY`: она строит индекс, не
// блокируя запись. Платит за это тем, что не умеет работать внутри транзакции. А наш раннер оборачивает
// каждую миграцию в транзакцию и на такой файл падает с «cannot run inside a transaction block».
// Договор: если файл начинается со строки `-- no-transaction`, раннер выполняет его команды по одной,
// вне транзакции (в одном файле их может быть несколько), и потом записывает версию. Остальные
// файлы остаются транзакционными.
// TODO: Научи `migrate` пропускать транзакцию для таких файлов.

use sqlx::PgPool;

async fn migrate(pool: &PgPool, files: &[(i64, &str, &str)]) -> Result<usize, sqlx::Error> {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations (version bigint PRIMARY KEY, name text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())")
        .execute(pool)
        .await?;
    let done: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations").fetch_all(pool).await?;
    let mut applied = 0;
    for (version, name, sql) in files {
        if done.contains(version) {
            continue;
        }
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        applied += 1;
    }
    Ok(applied)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const BASE: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author_id bigint, created_at timestamptz)";

    async fn valid_indexes(db: &TestDb) -> Vec<String> {
        let t = db.table("SELECT c.relname FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid WHERE i.indrelid = 'posts'::regclass AND i.indisvalid AND c.relname LIKE 'posts_%_idx' ORDER BY 1").await;
        t.column(0).into_iter().map(String::from).collect()
    }

    #[tokio::test]
    async fn concurrent_indexes_are_built_outside_a_transaction() {
        let db = TestDb::with_sql(BASE).await;
        let files = [
            (1, "no_tx", "-- no-transaction\nCREATE INDEX CONCURRENTLY posts_author_idx ON posts (author_id);\nCREATE INDEX CONCURRENTLY posts_created_idx ON posts (created_at);"),
        ];
        let n = migrate(&db.pool, &files).await.expect("файл с пометкой `-- no-transaction` должен выполняться вне транзакции");
        assert_eq!(n, 1, "применён один файл");
        assert_eq!(valid_indexes(&db).await, vec!["posts_author_idx", "posts_created_idx"], "оба индекса должны быть созданы и пригодны");
        let t = db.table("SELECT version FROM schema_migrations").await;
        assert_eq!(t.col("version"), vec!["1"], "версия должна быть записана после команд");
    }

    #[tokio::test]
    async fn without_the_marker_it_fails_and_is_not_recorded() {
        let db = TestDb::with_sql(BASE).await;
        let files = [(1, "oops", "CREATE INDEX CONCURRENTLY posts_author_idx ON posts (author_id)"), (2, "later", "CREATE TABLE later (id int)")];
        let res = migrate(&db.pool, &files).await;
        assert!(res.is_err(), "CREATE INDEX CONCURRENTLY внутри обычной транзакционной миграции должен падать");
        let t = db.table("SELECT count(*) FROM schema_migrations").await;
        assert_eq!(t.cell(0, 0), "0", "упавшая миграция не записывается, следующие не применяются");
    }

    #[tokio::test]
    async fn ordinary_files_stay_transactional() {
        let db = TestDb::with_sql(BASE).await;
        let files = [
            (1, "no_tx", "-- no-transaction\nCREATE INDEX CONCURRENTLY posts_author_idx ON posts (author_id)"),
            (2, "broken", "CREATE TABLE extra (id int); SELECT 1/0"),
        ];
        assert!(migrate(&db.pool, &files).await.is_err(), "вторая миграция делит на ноль");
        let t = db.table("SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = 'extra'").await;
        assert_eq!(t.cell(0, 0), "0", "обычная миграция остаётся атомарной даже рядом с файлами без транзакции");
        let t = db.table("SELECT version FROM schema_migrations ORDER BY version").await;
        assert_eq!(t.col("version"), vec!["1"], "первая миграция записана, вторая нет");
    }
}
