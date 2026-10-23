use sqlx::PgPool;

async fn import_users(pool: &PgPool, names: &[&str]) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut added = 0;
    for name in names {
        added += sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(name)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    sqlx::query("INSERT INTO audit (note) VALUES ($1)")
        .bind(format!("import: {added} of {}", names.len()))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(added)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn first_import_is_audited() {
        let db = TestDb::with_sql(SEED).await;
        let added = import_users(&db.pool, &["eva", "fedor", "eva"]).await.expect("импорт должен пройти");
        assert_eq!(added, 2, "eva встречается дважды, добавиться должны два ника");
        let t = db.table("SELECT note FROM audit").await;
        assert_eq!(t.len(), 1, "про импорт должна появиться одна запись в audit:\n{t}");
        assert_eq!(t.cell(0, 0), "import: 2 of 3", "в записи должно быть число добавленных и присланных ников");
    }

    #[tokio::test]
    async fn repeated_import_is_audited_too() {
        let db = TestDb::with_sql(SEED).await;
        import_users(&db.pool, &["eva", "fedor"]).await.expect("первый импорт");
        let added = import_users(&db.pool, &["eva", "fedor", "anna"]).await.expect("повторный импорт");
        assert_eq!(added, 0, "все три ника уже заняты, добавляться нечему");
        let t = db.table("SELECT note FROM audit ORDER BY id").await;
        assert_eq!(t.len(), 2, "повторная попытка тоже должна оставить запись в audit, а их:\n{t}");
        assert_eq!(t.cell(1, 0), "import: 0 of 3", "вторая запись должна честно сказать, что добавлено 0");
        let users = db.table("SELECT count(*) FROM users").await;
        assert_eq!(users.cell(0, 0), "6", "должно быть 4 старых пользователя и 2 новых");
    }
}
