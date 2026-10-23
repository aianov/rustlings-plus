use contracts::{Cause, Origin, UserId, Username};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use user_service::repository::{LifecycleRepository, OutboxRepository, RepoError, UserRepository};
use user_service::{MIGRATIONS, PgUsers};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    let schema = format!("t_users_{name}_{}", std::process::id());
    DbConfig::new(&url, &schema).pool(2)
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m51"), 1)
}

#[tokio::test]
async fn users_live_in_postgres() {
    let config = database("live");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    assert_eq!(apply(&pool, &MIGRATIONS).await.expect("миграции"), 2);
    assert_eq!(
        apply(&pool, &MIGRATIONS).await.expect("повтор миграций"),
        2,
        "каждый файл идемпотентен"
    );
    let users = PgUsers::new(pool.clone());
    let alice = users
        .create_if_absent(name("alice"), &cause())
        .await
        .expect("alice");
    let bob = users
        .create_if_absent(name("bob"), &cause())
        .await
        .expect("bob");
    assert!(
        matches!(
            users.create_if_absent(name("alice"), &cause()).await,
            Err(RepoError::Conflict)
        ),
        "ник занят: это имя ограничения users_username_key"
    );
    assert!(
        users
            .follow(bob.id, alice.id, &cause())
            .await
            .expect("подписка")
    );
    assert!(
        !users
            .follow(bob.id, alice.id, &cause())
            .await
            .expect("повтор")
    );
    assert!(
        users
            .mute(bob.id, alice.id, &cause())
            .await
            .expect("скрытие")
    );
    let relations = users.relations(alice.id).await;
    assert_eq!(relations.followers, [bob.id]);
    assert!(
        relations.audience.is_empty(),
        "bob скрыл alice: в аудитории его нет"
    );
    let rows = users.unpublished(10).await;
    let kinds: Vec<&str> = rows.iter().map(|row| row.event.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "user.registered",
            "user.registered",
            "user.followed",
            "user.muted"
        ]
    );
    assert_eq!(
        rows[2].event.version, 2,
        "версия считается по агрегату: у bob это вторая запись после регистрации"
    );
    users.mark_published(rows[1].seq).await.expect("отметка");
    assert_eq!(users.waiting().await, 2);
    let again = PgUsers::new(pool.clone());
    assert_eq!(
        again.find(name("alice")).await.map(|user| user.id),
        Some(alice.id),
        "данные переживают перезапуск"
    );
    assert!(again.hide(alice.id, &cause()).await.expect("скрыть"));
    assert!(again.get(alice.id).await.is_none());
    assert!(again.show(alice.id, &cause()).await.expect("вернуть"));
    assert!(again.erase(alice.id, &cause()).await.expect("стереть"));
    assert!(
        again.relations(bob.id).await.following.is_empty(),
        "подписки стёртого ушли"
    );
    let reborn = again
        .create_if_absent(name("alice"), &cause())
        .await
        .expect("ник освободился");
    assert_ne!(reborn.id, alice.id);
    assert!(again.get(UserId(999_999)).await.is_none());
    pool.close().await;
    drop_schema(&config).await.expect("схема убрана");
}
