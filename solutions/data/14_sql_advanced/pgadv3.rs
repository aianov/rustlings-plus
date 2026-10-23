fn set_theme() -> &'static str {
    "UPDATE profiles SET settings = jsonb_set(settings, '{theme}', to_jsonb($2::text)) WHERE user_id = $1"
}

fn set_email() -> &'static str {
    "UPDATE profiles SET settings = jsonb_set(settings, '{notifications}', COALESCE(settings -> 'notifications', '{}'::jsonb) || jsonb_build_object('email', $2::boolean)) WHERE user_id = $1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use serde_json::{Value, json};

    const SEED: &str = r#"
        CREATE TABLE profiles (user_id bigint PRIMARY KEY, settings jsonb NOT NULL);
        INSERT INTO profiles VALUES
            (1, '{"theme":"dark","ui":{"lang":"ru"},"notifications":{"email":true,"push":true}}'),
            (2, '{"theme":"light","notifications":{"push":true}}'),
            (3, '{"theme":"dark"}');
    "#;

    async fn settings_of(db: &TestDb, id: i64) -> Value {
        let t = db.table(&format!("SELECT settings::text FROM profiles WHERE user_id = {id}")).await;
        serde_json::from_str(t.cell(0, 0)).expect("settings должен быть JSON")
    }

    #[tokio::test]
    async fn theme_changes_and_the_rest_stays() {
        let db = TestDb::with_sql(SEED).await;
        let done = sqlx::query(set_theme()).bind(1i64).bind("light").execute(&db.pool).await.expect("set_theme");
        assert_eq!(done.rows_affected(), 1, "обновляется ровно один пользователь");
        assert_eq!(
            settings_of(&db, 1).await,
            json!({"theme": "light", "ui": {"lang": "ru"}, "notifications": {"email": true, "push": true}}),
            "после смены темы язык и уведомления должны остаться на месте"
        );
        assert_eq!(settings_of(&db, 3).await, json!({"theme": "dark"}), "чужие настройки трогать нельзя");
    }

    #[tokio::test]
    async fn email_switches_inside_an_existing_section() {
        let db = TestDb::with_sql(SEED).await;
        sqlx::query(set_email()).bind(1i64).bind(false).execute(&db.pool).await.expect("set_email");
        assert_eq!(
            settings_of(&db, 1).await,
            json!({"theme": "dark", "ui": {"lang": "ru"}, "notifications": {"email": false, "push": true}}),
            "поменяться должен только email, push остаётся"
        );
        sqlx::query(set_email()).bind(2i64).bind(true).execute(&db.pool).await.expect("set_email");
        assert_eq!(settings_of(&db, 2).await, json!({"theme": "light", "notifications": {"push": true, "email": true}}), "раздел есть, а email в нём нет: добавить");
    }

    #[tokio::test]
    async fn email_creates_the_missing_section() {
        let db = TestDb::with_sql(SEED).await;
        sqlx::query(set_email()).bind(3i64).bind(true).execute(&db.pool).await.expect("set_email");
        assert_eq!(
            settings_of(&db, 3).await,
            json!({"theme": "dark", "notifications": {"email": true}}),
            "у пользователя 3 раздела notifications не было: jsonb_set по вложенному пути молча ничего не делает, пока нет родителя"
        );
    }
}
