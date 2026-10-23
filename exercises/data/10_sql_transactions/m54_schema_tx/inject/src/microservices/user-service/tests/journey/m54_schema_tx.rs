use std::sync::Arc;

use contracts::{Cause, Origin, Username};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use user_service::repository::{SocialQueries, UserRepository};
use user_service::{MIGRATIONS, PgUsers};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    let schema = format!("t_counts_{name}_{}", std::process::id());
    DbConfig::new(&url, &schema).pool(8)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m54"), 1)
}

#[tokio::test]
async fn crossing_follows_keep_counters_without_deadlocks() {
    let config = database("cross");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    apply(&pool, &MIGRATIONS).await.expect("повтор миграций");
    let users = Arc::new(PgUsers::new(pool.clone()));
    let mut ids = Vec::new();
    for n in 0..6 {
        let name = Username::try_from(format!("user{n}").as_str()).expect("ник");
        ids.push(
            users
                .create_if_absent(name, &cause())
                .await
                .expect("пользователь")
                .id,
        );
    }
    let mut tasks = Vec::new();
    for (a, who) in ids.iter().enumerate() {
        for (b, whom) in ids.iter().enumerate() {
            if a == b {
                continue;
            }
            let users = Arc::clone(&users);
            let (who, whom) = (*who, *whom);
            tasks.push(tokio::spawn(async move {
                users.follow(who, whom, &cause()).await
            }));
        }
    }
    for task in tasks {
        assert!(
            task.await
                .expect("задача")
                .expect("встречные подписки без дедлока"),
            "каждая подписка новая"
        );
    }
    for id in &ids {
        assert_eq!(
            users.counters(*id).await,
            (5, 5),
            "счётчики рядом с данными"
        );
    }
    assert!(
        !users
            .follow(ids[0], ids[1], &cause())
            .await
            .expect("повтор")
    );
    assert_eq!(
        users.counters(ids[1]).await,
        (5, 5),
        "повтор не двигает счётчик"
    );
    let twin = sqlx::query("INSERT INTO users (username) VALUES ('USER0')")
        .execute(&pool)
        .await;
    let code = twin.err().and_then(|error| {
        error
            .as_database_error()
            .and_then(|db| db.code().map(|c| c.to_string()))
    });
    assert_eq!(code.as_deref(), Some("23505"), "ник уникален по lower()");
    let _ = drop_schema(&config).await;
}
