use contracts::{Cause, Origin, Username};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema, migrate};
use user_service::common::Settings;
use user_service::repository::{ProfileExtras, UserRepository};
use user_service::{MIGRATIONS, PgUsers};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    let schema = format!("t_people_{name}_{}", std::process::id());
    DbConfig::new(&url, &schema).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m58"), 1)
}

#[tokio::test]
async fn people_with_typos_and_jsonb_settings() {
    let config = database("typos");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let users = PgUsers::new(pool.clone());
    let mut ids = Vec::new();
    for nick in ["alice", "alicia", "bob"] {
        let name = Username::try_from(nick).expect("ник");
        ids.push(users.create_if_absent(name, &cause()).await.expect(nick).id);
    }
    let found: Vec<String> = users
        .people(String::from("alise"), 10)
        .await
        .iter()
        .map(|user| user.name.as_ref().to_string())
        .collect();
    assert_eq!(
        found.first().map(String::as_str),
        Some("alice"),
        "опечатка прощена: {found:?}"
    );
    assert!(!found.contains(&String::from("bob")));
    let before: String = sqlx::query_scalar("SELECT updated_at::text FROM users WHERE id = $1")
        .bind(i64::try_from(ids[0].0).expect("id"))
        .fetch_one(&pool)
        .await
        .expect("время");
    let dark = Settings {
        theme: Some(String::from("dark")),
        ..Settings::default()
    };
    users.patch_settings(ids[0], dark).await.expect("тема");
    let english = Settings {
        language: Some(String::from("en")),
        ..Settings::default()
    };
    let merged = users.patch_settings(ids[0], english).await.expect("язык");
    assert_eq!(
        (merged.theme.as_deref(), merged.language.as_deref()),
        (Some("dark"), Some("en")),
        "частичное обновление jsonb ||"
    );
    assert_eq!(users.settings(ids[1]).await, Some(Settings::default()));
    let after: String = sqlx::query_scalar("SELECT updated_at::text FROM users WHERE id = $1")
        .bind(i64::try_from(ids[0].0).expect("id"))
        .fetch_one(&pool)
        .await
        .expect("время");
    assert_ne!(before, after, "триггер updated_at");
    let _ = drop_schema(&config).await;
}
