fn save_theme() -> &'static str {
    "INSERT INTO settings (user_id, theme) VALUES ($1, $2)
     ON CONFLICT (user_id) DO UPDATE SET theme = EXCLUDED.theme, updated_at = now()"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    async fn save(db: &TestDb, user: i64, theme: &str) {
        sqlx::query(save_theme())
            .bind(user)
            .bind(theme)
            .execute(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("сохранение темы {theme} для пользователя {user} не удалось: {e}"));
    }

    #[tokio::test]
    async fn creates_then_updates() {
        let db = TestDb::with_sql(SEED).await;
        save(&db, 3, "dark").await;
        let first = db.table("SELECT theme, updated_at::text FROM settings WHERE user_id = 3").await;
        assert_eq!(first.cell(0, 0), "dark", "первое сохранение создаёт запись:\n{first}");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        save(&db, 3, "light").await;
        let t = db.table("SELECT theme, updated_at::text FROM settings WHERE user_id = 3").await;
        assert_eq!(t.len(), 1, "запись должна остаться одна:\n{t}");
        assert_eq!(t.cell(0, 0), "light", "второе сохранение меняет тему:\n{t}");
        assert_ne!(t.cell(0, 1), first.cell(0, 1), "updated_at должен обновиться при изменении темы");
    }

    #[tokio::test]
    async fn users_do_not_share_rows() {
        let db = TestDb::with_sql(SEED).await;
        save(&db, 1, "dark").await;
        save(&db, 2, "light").await;
        save(&db, 1, "blue").await;
        let t = db.table("SELECT user_id, theme FROM settings ORDER BY user_id").await;
        assert_eq!(t.rows, [["1", "blue"], ["2", "light"]], "у каждого пользователя своя запись:\n{t}");
    }
}
