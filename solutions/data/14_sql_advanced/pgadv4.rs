use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::types::Json;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Notifications {
    email: bool,
    push: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Settings {
    theme: String,
    #[serde(default)]
    beta: bool,
    notifications: Notifications,
}

async fn load(pool: &PgPool, user_id: i64) -> Result<Option<Settings>, sqlx::Error> {
    let found: Option<Json<Settings>> = sqlx::query_scalar("SELECT settings FROM profiles WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.map(|j| j.0))
}

async fn save(pool: &PgPool, user_id: i64, settings: &Settings) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO profiles (user_id, settings) VALUES ($1, $2) ON CONFLICT (user_id) DO UPDATE SET settings = EXCLUDED.settings")
        .bind(user_id)
        .bind(Json(settings))
        .execute(pool)
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = r#"
        CREATE TABLE profiles (user_id bigint PRIMARY KEY, settings jsonb NOT NULL);
        INSERT INTO profiles VALUES (1, '{"theme":"dark","notifications":{"email":true,"push":false},"future_flag":42}');
    "#;

    fn sample(theme: &str, beta: bool) -> Settings {
        Settings { theme: theme.to_string(), beta, notifications: Notifications { email: false, push: true } }
    }

    #[tokio::test]
    async fn loads_a_row_with_missing_and_extra_keys() {
        let db = TestDb::with_sql(SEED).await;
        let got = load(&db.pool, 1).await.expect("load не должен падать на документе с лишним ключом future_flag и без beta");
        let expected = Settings { theme: "dark".into(), beta: false, notifications: Notifications { email: true, push: false } };
        assert_eq!(got, Some(expected), "настройки пользователя 1 прочитаны неверно");
        assert_eq!(load(&db.pool, 99).await.expect("load"), None, "нет строки: None, а не ошибка");
    }

    #[tokio::test]
    async fn saves_new_and_replaces_existing() {
        let db = TestDb::with_sql(SEED).await;
        save(&db.pool, 2, &sample("light", true)).await.expect("save нового пользователя");
        assert_eq!(load(&db.pool, 2).await.expect("load"), Some(sample("light", true)), "после save настройки читаются обратно");
        save(&db.pool, 2, &sample("auto", false)).await.expect("повторный save того же пользователя заменяет настройки");
        assert_eq!(load(&db.pool, 2).await.expect("load"), Some(sample("auto", false)), "вторая запись должна заменить первую");
        let t = db.table("SELECT count(*) FROM profiles WHERE user_id = 2").await;
        assert_eq!(t.cell(0, 0), "1", "строка пользователя должна остаться одна");
    }

    #[tokio::test]
    async fn stores_an_object_not_a_json_string() {
        let db = TestDb::with_sql(SEED).await;
        save(&db.pool, 3, &sample("dark", true)).await.expect("save");
        let t = db.table("SELECT jsonb_typeof(settings), settings -> 'notifications' ->> 'push' FROM profiles WHERE user_id = 3").await;
        assert_eq!(t.rows, vec![vec!["object", "true"]], "в столбце должен лежать объект JSON, а не строка с экранированным JSON внутри:\n{t}");
    }
}
