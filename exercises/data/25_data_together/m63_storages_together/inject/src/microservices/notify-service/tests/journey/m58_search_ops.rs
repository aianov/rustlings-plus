use contracts::{NoticeKind, PostId, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema, migrate};
use notify_service::common::Draft;
use notify_service::repository::{MIGRATIONS, PgNotices};
use notify_service::{NoticeRepository, Recorded};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_notify_{name}_{}", std::process::id())).pool(2)
}

fn liked(to: u64, actor: u64, post: u64) -> Draft {
    Draft {
        to: UserId(to),
        kind: NoticeKind::Liked,
        actor: UserId(actor),
        post: Some(PostId(post)),
        count: None,
    }
}

#[tokio::test]
async fn notices_live_in_monthly_partitions() {
    let config = database("parts");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let notices = PgNotices::new(pool.clone());
    assert!(matches!(
        notices.record(liked(1, 2, 10)).await,
        Recorded::Stored(..)
    ));
    assert_eq!(notices.record(liked(1, 2, 10)).await, Recorded::Duplicate);
    assert!(matches!(
        notices.record(liked(1, 3, 10)).await,
        Recorded::Stored(..)
    ));
    assert_eq!(notices.unread(UserId(1)).await, 2);
    assert_eq!(
        notices.list(UserId(1), 10).await[0].actor,
        UserId(3),
        "новые первыми"
    );
    assert_eq!(notices.unread_counts().await, [(UserId(1), 2)]);
    assert_eq!(notices.mark_read(UserId(1)).await, 2);
    assert_eq!(notices.unread(UserId(1)).await, 0);
    let made = notices.ensure_partitions(6, 1).await.expect("партиции");
    assert!(made >= 6, "партиции назад и вперёд: {made}");
    sqlx::query("INSERT INTO notices (to_user, kind, actor, at) VALUES (7, 'followed', 8, now() - interval '5 months')")
        .execute(&pool)
        .await
        .expect("старое уведомление в своей партиции");
    let dropped = notices.drop_older(3).await.expect("удаление");
    assert!(dropped >= 2, "старые партиции целиком: {dropped}");
    assert!(
        notices.list(UserId(7), 10).await.is_empty(),
        "старое ушло с партицией"
    );
    assert_eq!(
        notices.list(UserId(1), 10).await.len(),
        2,
        "свежее на месте"
    );
    let _ = drop_schema(&config).await;
}

#[tokio::test]
async fn the_inbox_takes_each_event_once_in_the_same_transaction() {
    let config = database("inbox");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let notices = PgNotices::new(pool.clone());
    let event = Some(String::from("ev-63"));
    assert!(matches!(
        notices.record_once(event.clone(), liked(1, 2, 10)).await,
        Recorded::Stored(..)
    ));
    assert_eq!(
        notices.record_once(event, liked(1, 2, 10)).await,
        Recorded::Duplicate,
        "повторная доставка того же события отсечена inbox"
    );
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM inbox")
        .fetch_one(&pool)
        .await
        .expect("inbox");
    assert_eq!(kept, 1);
    assert_eq!(notices.unread(UserId(1)).await, 1);
    let _ = drop_schema(&config).await;
}
