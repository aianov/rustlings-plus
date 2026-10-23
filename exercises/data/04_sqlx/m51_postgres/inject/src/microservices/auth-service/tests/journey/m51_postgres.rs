use auth_service::common::Account;
use auth_service::repository::{
    AccountRepository, OneTime, OneTimeRepository, Opening, Purpose, RefreshError, RepoError,
    SessionRepository,
};
use auth_service::{MIGRATIONS, PgAccounts};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_auth_{name}_{}", std::process::id())).pool(2)
}

fn account(id: u64, name: &str) -> Account {
    Account {
        user_id: id,
        username: name.to_string(),
        email: None,
        email_verified: false,
        password: Some(String::from("$argon2id$хеш")),
        role: String::from("user"),
        frozen: false,
        deleted: false,
    }
}

fn opening(hash: &str, jti: &str) -> Opening {
    Opening {
        refresh_hash: hash.to_string(),
        access_jti: jti.to_string(),
        access_exp: 1_000,
        expires: 10_000,
    }
}

#[tokio::test]
async fn accounts_and_sessions_live_in_postgres() {
    let config = database("live");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let store = PgAccounts::new(pool.clone(), 30);
    store
        .create_if_absent(account(1, "Alice"))
        .await
        .expect("alice");
    assert!(
        matches!(
            store.create_if_absent(account(2, "alice")).await,
            Err(RepoError::Conflict)
        ),
        "ник без учёта регистра занят: accounts_username_key"
    );
    assert_eq!(
        store
            .find(String::from("ALICE"))
            .await
            .map(|own| own.user_id),
        Some(1)
    );
    let mut frozen = account(1, "Alice");
    frozen.frozen = true;
    store.update(frozen).await.expect("заморозка");
    assert!(store.get(1).await.is_some_and(|own| own.frozen));
    let first = store.open(1, opening("r1", "a1")).await;
    let second = store
        .rotate(String::from("r1"), 5, opening("r2", "a2"))
        .await
        .expect("обмен");
    assert_eq!(second.family, first.family, "ротация остаётся в семействе");
    assert_eq!(
        store
            .rotate(String::from("r1"), 6, opening("r3", "a3"))
            .await
            .err(),
        Some(RefreshError::Reused),
        "повтор старого refresh"
    );
    assert!(
        store.is_denied(String::from("a2")).await,
        "повтор погасил всё семейство"
    );
    assert_eq!(
        store
            .rotate(String::from("r2"), 7, opening("r4", "a4"))
            .await
            .err(),
        Some(RefreshError::Revoked)
    );
    let other = store.open(1, opening("r5", "a5")).await;
    assert_eq!(store.revoke_user(1, None).await, [other.family]);
    store
        .issue(
            String::from("h1"),
            OneTime {
                purpose: Purpose::ResetPassword,
                user_id: 1,
                expires: 100,
            },
        )
        .await;
    assert_eq!(
        store
            .take(Purpose::ConfirmEmail, String::from("h1"), 1)
            .await,
        None
    );
    assert_eq!(
        store
            .take(Purpose::ResetPassword, String::from("h1"), 1)
            .await,
        Some(1)
    );
    assert_eq!(
        store
            .take(Purpose::ResetPassword, String::from("h1"), 1)
            .await,
        None,
        "одноразовый"
    );
    assert!(store.purge(20_000).await >= 2);
    pool.close().await;
    drop_schema(&config).await.expect("схема убрана");
}
