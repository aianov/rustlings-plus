use sqlx::PgPool;

async fn migrate(pool: &PgPool, files: &[(i64, &str, &str)]) -> Result<usize, sqlx::Error> {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations (version bigint PRIMARY KEY, name text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())")
        .execute(pool)
        .await?;
    let done: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations").fetch_all(pool).await?;
    let mut pending: Vec<&(i64, &str, &str)> = files.iter().filter(|f| !done.contains(&f.0)).collect();
    pending.sort_by_key(|f| f.0);
    for (version, name, sql) in &pending {
        sqlx::raw_sql(sql).execute(pool).await?;
        sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(pool)
            .await?;
    }
    Ok(pending.len())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    fn files() -> Vec<(i64, &'static str, &'static str)> {
        vec![
            (2, "add_title", "ALTER TABLE books ADD COLUMN title text NOT NULL DEFAULT ''"),
            (1, "create_books", "CREATE TABLE books (id bigint PRIMARY KEY); CREATE INDEX books_id_idx ON books (id)"),
            (3, "first_book", "INSERT INTO books (id, title) VALUES (1, 'Dune')"),
        ]
    }

    #[tokio::test]
    async fn applies_in_version_order() {
        let db = TestDb::new().await;
        let n = migrate(&db.pool, &files()).await.expect("миграции должны применяться: версия 2 зависит от версии 1, а в списке она стоит позже");
        assert_eq!(n, 3, "применены должны быть все три файла");
        let t = db.table("SELECT version, name FROM schema_migrations ORDER BY version").await;
        assert_eq!(t.rows, vec![vec!["1", "create_books"], vec!["2", "add_title"], vec!["3", "first_book"]], "в таблице версий должны быть три записи с именами:\n{t}");
        let t = db.table("SELECT title FROM books").await;
        assert_eq!(t.col("title"), vec!["Dune"], "данные третьей миграции должны быть на месте");
    }

    #[tokio::test]
    async fn second_run_applies_nothing() {
        let db = TestDb::new().await;
        migrate(&db.pool, &files()).await.expect("первый запуск");
        let n = migrate(&db.pool, &files()).await.expect("повторный запуск не должен падать: все версии уже записаны");
        assert_eq!(n, 0, "повторный запуск ничего не применяет");
    }

    #[tokio::test]
    async fn new_file_is_picked_up_later() {
        let db = TestDb::new().await;
        let mut all = files();
        let last = all.pop().unwrap();
        assert_eq!(migrate(&db.pool, &all).await.expect("первые две"), 2, "сначала две миграции");
        all.push(last);
        assert_eq!(migrate(&db.pool, &all).await.expect("с третьей"), 1, "при добавлении третьего файла применяется только он");
    }
}
