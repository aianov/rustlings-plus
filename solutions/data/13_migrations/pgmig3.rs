use sqlx::PgPool;

#[derive(Debug)]
enum MigrateError {
    Drift(i64),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for MigrateError {
    fn from(e: sqlx::Error) -> MigrateError {
        MigrateError::Db(e)
    }
}

async fn migrate(pool: &PgPool, files: &[(i64, &str, &str)]) -> Result<usize, MigrateError> {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations (version bigint PRIMARY KEY, name text NOT NULL, checksum text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())")
        .execute(pool)
        .await?;
    let recorded: Vec<(i64, String)> = sqlx::query_as("SELECT version, checksum FROM schema_migrations").fetch_all(pool).await?;
    for (version, checksum) in &recorded {
        if let Some((_, _, sql)) = files.iter().find(|f| f.0 == *version) {
            let current: String = sqlx::query_scalar("SELECT md5($1)").bind(sql).fetch_one(pool).await?;
            if &current != checksum {
                return Err(MigrateError::Drift(*version));
            }
        }
    }
    let mut applied = 0;
    for (version, name, sql) in files {
        if recorded.iter().any(|r| r.0 == *version) {
            continue;
        }
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_migrations (version, name, checksum) VALUES ($1, $2, md5($3))")
            .bind(version)
            .bind(name)
            .bind(sql)
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

    #[tokio::test]
    async fn untouched_files_pass_the_check() {
        let db = TestDb::new().await;
        let files = [(1, "users", "CREATE TABLE users (id int)"), (2, "posts", "CREATE TABLE posts (id int)")];
        assert_eq!(migrate(&db.pool, &files).await.expect("первый запуск"), 2, "применены обе");
        assert_eq!(migrate(&db.pool, &files).await.expect("повторный запуск без правок"), 0, "ничего не изменилось, проверка должна пройти");
    }

    #[tokio::test]
    async fn edited_file_is_caught_before_anything_new_runs() {
        let db = TestDb::new().await;
        let files = [(1, "users", "CREATE TABLE users (id int)")];
        migrate(&db.pool, &files).await.expect("первый запуск");
        let edited = [(1, "users", "CREATE TABLE users (id bigint)"), (2, "posts", "CREATE TABLE posts (id int)")];
        match migrate(&db.pool, &edited).await {
            Err(MigrateError::Drift(v)) => assert_eq!(v, 1, "правили версию 1"),
            Err(MigrateError::Db(e)) => panic!("ожидалась ошибка расхождения Drift(1), а пришла ошибка базы: {e}"),
            Ok(n) => panic!("правку уже применённого файла надо замечать, а применено {n}"),
        }
        let t = db.table("SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = 'posts'").await;
        assert_eq!(t.cell(0, 0), "0", "пока есть расхождение, новые миграции применять нельзя");
    }

    #[tokio::test]
    async fn whitespace_change_is_still_a_change() {
        let db = TestDb::new().await;
        migrate(&db.pool, &[(1, "users", "CREATE TABLE users (id int)")]).await.expect("первый запуск");
        let res = migrate(&db.pool, &[(1, "users", "CREATE TABLE users (id int) ")]).await;
        assert!(matches!(res, Err(MigrateError::Drift(1))), "любое изменение текста меняет сумму");
    }
}
