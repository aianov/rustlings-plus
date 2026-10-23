// В Rust настройки профиля это обычная структура, а в таблице столбец jsonb. Переводить руками через
// строки и `serde_json::from_str` скучно и опасно: легко записать JSON как текст, и столбец получит
// не объект, а строку с кавычками. Для таких случаев sqlx умеет `Json<T>`: оборачиваешь структуру, а
// кодирование и разбор он делает сам. Пользователя могут не оказаться в таблице, а в документе
// бывают лишние ключи, которых структура не знает (их надо просто пропускать).
// TODO: Допиши `load` (нет строки: `None`) и `save` (вставка или замена настроек пользователя).

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
    let _ = (pool, user_id);
    Ok(None)
}

async fn save(pool: &PgPool, user_id: i64, settings: &Settings) -> Result<(), sqlx::Error> {
    let _ = (pool, user_id, settings);
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
