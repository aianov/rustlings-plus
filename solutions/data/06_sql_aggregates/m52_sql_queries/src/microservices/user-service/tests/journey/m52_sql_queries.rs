use contracts::{Cause, Origin, Username};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use user_service::repository::{SocialQueries, UserRepository};
use user_service::{MIGRATIONS, PgUsers};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    let schema = format!("t_social_{name}_{}", std::process::id());
    DbConfig::new(&url, &schema).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m52"), 1)
}

fn names(users: &[user_service::common::User]) -> Vec<String> {
    users
        .iter()
        .map(|user| user.name.as_ref().to_string())
        .collect()
}

#[tokio::test]
async fn followers_friends_and_counters_come_from_sql() {
    let config = database("graph");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let users = PgUsers::new(pool);
    let mut ids = Vec::new();
    for nick in ["alice", "bob", "carol"] {
        let name = Username::try_from(nick).expect("ник");
        ids.push(users.create_if_absent(name, &cause()).await.expect(nick).id);
    }
    let (alice, bob, carol) = (ids[0], ids[1], ids[2]);
    users
        .follow(alice, bob, &cause())
        .await
        .expect("alice → bob");
    users
        .follow(bob, alice, &cause())
        .await
        .expect("bob → alice");
    users
        .follow(carol, alice, &cause())
        .await
        .expect("carol → alice");
    assert_eq!(
        names(&users.followers(alice, 0, 10).await),
        ["bob", "carol"]
    );
    assert_eq!(names(&users.following(alice, 0, 10).await), ["bob"]);
    assert_eq!(
        names(&users.friends(alice, 0, 10).await),
        ["bob"],
        "друзья: подписка в обе стороны"
    );
    assert!(users.friends(carol, 0, 10).await.is_empty());
    assert_eq!(
        names(&users.followers(alice, bob.0, 10).await),
        ["carol"],
        "курсор after продолжает список"
    );
    assert_eq!(users.counters(alice).await, (2, 1));
    assert_eq!(users.counters(carol).await, (0, 1));
    let _ = drop_schema(&config).await;
}
