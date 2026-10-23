use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{Duration, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use sqlx_user_repo::{NewUser, PgUserRepository, RepoError, UserRepository, apply_schema};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL")
        .unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

struct Env {
    pool: PgPool,
    schema: String,
    repo: PgUserRepository,
}

impl Env {
    async fn new() -> Env {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let schema = format!("repo_{}_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed), nanos);
        let mut conn = PgConnection::connect(&pg_url())
            .await
            .unwrap_or_else(|e| panic!("Postgres курса не отвечает ({}): {e}. Запусти docker compose up -d", pg_url()));
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\"")).execute(&mut conn).await.unwrap();
        let options = PgConnectOptions::from_str(&pg_url()).unwrap().options([("search_path", schema.as_str()), ("timezone", "UTC")]);
        let pool = PgPoolOptions::new().max_connections(8).connect_with(options).await.unwrap();
        apply_schema(&pool).await.expect("схема из schema.sql должна применяться");
        let repo = PgUserRepository::new(pool.clone());
        Env { pool, schema, repo }
    }

    async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let url = pg_url();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                if let Ok(mut conn) = PgConnection::connect(&url).await {
                    let _ = sqlx::raw_sql(&format!("DROP SCHEMA \"{schema}\" CASCADE")).execute(&mut conn).await;
                }
            });
        })
        .join();
    }
}

fn new_user(name: &str) -> NewUser {
    NewUser { username: name.to_string(), email: format!("{name}@example.com") }
}

#[tokio::test]
async fn create_returns_what_the_database_filled_in() {
    let env = Env::new().await;
    let before = Utc::now() - Duration::seconds(5);
    let alice = env.repo.create(new_user("alice")).await.expect("создание пользователя");
    assert_eq!((alice.id, alice.username.as_str(), alice.email.as_str()), (1, "alice", "alice@example.com"), "поля созданного пользователя: {alice:?}");
    assert_eq!(alice.bio, None, "bio у нового пользователя не задано");
    assert!(alice.created_at > before && alice.created_at < Utc::now() + Duration::seconds(5), "created_at должен проставить сервер, а получилось {}", alice.created_at);
    let bob = env.repo.create(new_user("bob")).await.unwrap();
    assert_eq!(bob.id, 2, "id выдаёт база по порядку");
}

#[tokio::test]
async fn unique_violations_become_domain_errors() {
    let env = Env::new().await;
    env.repo.create(new_user("alice")).await.unwrap();
    let dup_name = env.repo.create(NewUser { username: "alice".into(), email: "other@example.com".into() }).await;
    assert!(matches!(dup_name, Err(RepoError::UsernameTaken)), "ник занят, ждали UsernameTaken: {dup_name:?}");
    let dup_mail = env.repo.create(NewUser { username: "alicia".into(), email: "alice@example.com".into() }).await;
    assert!(matches!(dup_mail, Err(RepoError::EmailTaken)), "почта занята, ждали EmailTaken: {dup_mail:?}");
    let short = env.repo.create(NewUser { username: "al".into(), email: "al@example.com".into() }).await;
    assert!(matches!(short, Err(RepoError::Invalid)), "ник из двух символов нарушает CHECK, ждали Invalid: {short:?}");
}

#[tokio::test]
async fn concurrent_registrations_of_one_name() {
    let env = Env::new().await;
    let mut tasks = Vec::new();
    for i in 0..10 {
        let repo = env.repo.clone();
        tasks.push(tokio::spawn(async move {
            repo.create(NewUser { username: "contested".into(), email: format!("c{i}@example.com") }).await
        }));
    }
    let (mut ok, mut taken) = (0, 0);
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(RepoError::UsernameTaken) => taken += 1,
            Err(e) => panic!("в гонке допустимы только успех и UsernameTaken, а вышло {e:?}"),
        }
    }
    assert_eq!((ok, taken), (1, 9), "из десяти одновременных регистраций одного ника проходит ровно одна");
}

#[tokio::test]
async fn lookups() {
    let env = Env::new().await;
    let alice = env.repo.create(new_user("alice")).await.unwrap();
    assert_eq!(env.repo.find_by_id(alice.id).await.unwrap(), Some(alice.clone()), "поиск по id");
    assert_eq!(env.repo.find_by_username("alice".into()).await.unwrap(), Some(alice), "поиск по нику");
    assert_eq!(env.repo.find_by_id(404).await.expect("отсутствие это не ошибка"), None, "нет такого id");
    assert_eq!(env.repo.find_by_username("nobody".into()).await.unwrap(), None, "нет такого ника");
}

#[tokio::test]
async fn database_failure_is_not_not_found() {
    let env = Env::new().await;
    env.pool.close().await;
    let got = env.repo.find_by_id(1).await;
    assert!(matches!(got, Err(RepoError::Db(_))), "недоступная база это Db, а не «пользователя нет»: {got:?}");
}

#[tokio::test]
async fn users_by_ids_keeps_order() {
    let env = Env::new().await;
    for name in ["alice", "bob", "carol", "dave"] {
        env.repo.create(new_user(name)).await.unwrap();
    }
    let got = env.repo.users_by_ids(vec![3, 1, 4]).await.unwrap();
    let names: Vec<&str> = got.iter().map(|u| u.username.as_str()).collect();
    assert_eq!(names, ["carol", "alice", "dave"], "порядок как в списке");
    let got = env.repo.users_by_ids(vec![2, 99, 2]).await.unwrap();
    assert_eq!(got.len(), 1, "99 пропускается, повтор даёт одну запись: {got:?}");
    assert!(env.repo.users_by_ids(vec![]).await.expect("пустой список не ошибка").is_empty(), "по пустому списку пусто");
}

#[tokio::test]
async fn bio_update_and_delete() {
    let env = Env::new().await;
    let u = env.repo.create(new_user("alice")).await.unwrap();
    env.repo.update_bio(u.id, Some("Пишу про Rust".into())).await.expect("обновление bio");
    assert_eq!(env.repo.find_by_id(u.id).await.unwrap().unwrap().bio.as_deref(), Some("Пишу про Rust"), "bio записалось");
    env.repo.update_bio(u.id, None).await.unwrap();
    assert_eq!(env.repo.find_by_id(u.id).await.unwrap().unwrap().bio, None, "None стирает bio в NULL");
    let missing = env.repo.update_bio(777, Some("x".into())).await;
    assert!(matches!(missing, Err(RepoError::NotFound)), "обновление несуществующего пользователя это NotFound: {missing:?}");
    env.repo.add_post(u.id, "пост".into()).await.unwrap();
    env.repo.delete(u.id).await.expect("удаление");
    assert_eq!(env.scalar("SELECT count(*) FROM posts").await, 0, "посты удаляются вместе с пользователем");
    let again = env.repo.delete(u.id).await;
    assert!(matches!(again, Err(RepoError::NotFound)), "повторное удаление это NotFound: {again:?}");
}

#[tokio::test]
async fn posts_validation() {
    let env = Env::new().await;
    let u = env.repo.create(new_user("alice")).await.unwrap();
    let p = env.repo.add_post(u.id, "привет".into()).await.expect("пост");
    assert_eq!((p.id, p.author_id, p.body.as_str()), (1, u.id, "привет"), "созданный пост: {p:?}");
    let ghost = env.repo.add_post(9999, "кому".into()).await;
    assert!(matches!(ghost, Err(RepoError::NotFound)), "автора нет: нарушение внешнего ключа должно стать NotFound: {ghost:?}");
    let blank = env.repo.add_post(u.id, "   ".into()).await;
    assert!(matches!(blank, Err(RepoError::Invalid)), "пустой пост нарушает CHECK, ждали Invalid: {blank:?}");
}

#[tokio::test]
async fn posts_are_newest_first_with_stable_paging() {
    let env = Env::new().await;
    let u = env.repo.create(new_user("alice")).await.unwrap();
    sqlx::raw_sql(&format!(
        "INSERT INTO posts (author_id, body, created_at) VALUES
            ({id}, 'старый',    '2024-01-01 10:00:00+00'),
            ({id}, 'новее',     '2024-02-01 10:00:00+00'),
            ({id}, 'близнец-1', '2024-03-01 10:00:00+00'),
            ({id}, 'близнец-2', '2024-03-01 10:00:00+00')",
        id = u.id
    ))
    .execute(&env.pool)
    .await
    .unwrap();
    let first = env.repo.posts_of(u.id, 2, 0).await.unwrap();
    let bodies: Vec<&str> = first.iter().map(|p| p.body.as_str()).collect();
    assert_eq!(bodies, ["близнец-2", "близнец-1"], "сначала новые, при равной дате больший id первым");
    let second = env.repo.posts_of(u.id, 2, 2).await.unwrap();
    let bodies: Vec<&str> = second.iter().map(|p| p.body.as_str()).collect();
    assert_eq!(bodies, ["новее", "старый"], "вторая страница продолжает первую без повторов");
    assert!(env.repo.posts_of(u.id, 2, 4).await.unwrap().is_empty(), "дальше пусто");
    assert!(env.repo.posts_of(12345, 10, 0).await.unwrap().is_empty(), "у неизвестного автора постов нет, и это не ошибка");
}

#[tokio::test]
async fn registration_with_first_post_is_atomic() {
    let env = Env::new().await;
    let (user, post) = env.repo.register_with_first_post(new_user("alice"), "Всем привет".into()).await.expect("регистрация");
    assert_eq!(post.author_id, user.id, "первый пост принадлежит новому пользователю");
    assert_eq!(env.scalar("SELECT count(*) FROM posts").await, 1, "пост записан");

    let failed = env.repo.register_with_first_post(new_user("bob"), "  ".into()).await;
    assert!(matches!(failed, Err(RepoError::Invalid)), "пустой первый пост: ждали Invalid, а вышло {failed:?}");
    assert_eq!(env.scalar("SELECT count(*) FROM users WHERE username = 'bob'").await, 0, "bob не должен остаться: регистрация откатывается целиком");

    let taken = env.repo.register_with_first_post(new_user("alice"), "второй раз".into()).await;
    assert!(matches!(taken, Err(RepoError::UsernameTaken)), "ник занят: {taken:?}");
    assert_eq!(env.scalar("SELECT count(*) FROM posts").await, 1, "лишних постов быть не должно");

    let (bob, _) = env.repo.register_with_first_post(new_user("bob"), "теперь нормально".into()).await.expect("после отката ник свободен");
    assert_eq!(bob.username, "bob", "повторная регистрация после отката проходит");
}

#[tokio::test]
async fn sql_is_not_built_by_formatting() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/postgres.rs");
    let text = std::fs::read_to_string(path).expect("src/postgres.rs должен существовать");
    assert!(!text.contains("format!("), "в src/postgres.rs найден format!: запросы строятся константами с параметрами $n, а не форматированием строк");
}
