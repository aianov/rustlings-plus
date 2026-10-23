use std::sync::Arc;
use std::time::Duration;

use contracts::{Cause, Origin, Topics, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema, migrate};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use post_service::common::now_micros;
use post_service::repository::{OutboxRepository, PostRepository};
use post_service::{Draft, MIGRATIONS, PgPosts, PostText, Relay};

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

#[tokio::test]
async fn a_relay_falling_mid_batch_hands_its_claims_over() {
    let id = std::process::id();
    let config = DbConfig::new(
        &std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string()),
        &format!("t_relay_{id}"),
    )
    .pool(4);
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let writer = Arc::new(
        PgPosts::open(pool.clone(), 21)
            .await
            .expect("первая реплика"),
    );
    let second = Arc::new(
        PgPosts::open(pool.clone(), 22)
            .await
            .expect("вторая реплика"),
    );
    for (author, text) in [(1, "первый"), (2, "второй"), (1, "третий")] {
        writer
            .insert(
                UserId(author),
                format!("u{author}"),
                draft(text),
                &Cause::new(&Origin::new("m63"), now_micros()),
            )
            .await
            .expect("пост");
    }
    let fallen = db_engine::claim(&pool, "fallen-relay", 10, Duration::from_secs(1))
        .await
        .expect("захват упавшего релея");
    assert_eq!(fallen.len(), 3, "упавший релей успел захватить пачку");
    let topics = Topics::new(&format!("m63r{id}"));
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса запущен?");
    let relay = Relay::open(bus, topics.clone(), second.clone())
        .await
        .expect("релей второй реплики");
    assert_eq!(
        relay.pump().await,
        Ok(0),
        "чужой захват держится, SKIP LOCKED его не трогает"
    );
    tokio::time::sleep(Duration::from_millis(1_300)).await;
    assert_eq!(
        relay.pump().await,
        Ok(3),
        "срок захвата вышел: вторая реплика подбирает"
    );
    assert_eq!(
        relay.pump().await,
        Ok(0),
        "опубликованное второй раз не уходит"
    );
    assert_eq!(second.waiting().await, 0);
    let js = async_nats::jetstream::new(async_nats::connect(url()).await.expect("NATS"));
    let mut stream = js.get_stream(topics.stream()).await.expect("stream");
    assert_eq!(
        stream.info().await.expect("сведения").state.messages,
        3,
        "ровно три события"
    );
    let _ = js.delete_stream(topics.stream()).await;
    let _ = drop_schema(&config).await;
}

#[tokio::test]
async fn claims_keep_order_inside_an_aggregate_and_wake_the_relay() {
    let id = std::process::id();
    let config = DbConfig::new(
        &std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string()),
        &format!("t_order_{id}"),
    )
    .pool(4);
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool.clone(), 23).await.expect("репозиторий");
    let woken = posts.signal();
    let first = posts
        .insert(
            UserId(1),
            String::from("a"),
            draft("пост"),
            &Cause::new(&Origin::new("m63"), now_micros()),
        )
        .await
        .expect("пост");
    tokio::time::timeout(Duration::from_secs(3), woken.notified())
        .await
        .expect("NOTIFY разбудил релей без опроса");
    posts
        .like_if_absent(
            UserId(2),
            first.id,
            &Cause::new(&Origin::new("m63"), now_micros()),
        )
        .await
        .expect("лайк")
        .expect("пост есть");
    let a = db_engine::claim(&pool, "relay-a", 1, Duration::from_secs(30))
        .await
        .expect("захват");
    assert_eq!(a.len(), 1);
    let b = db_engine::claim(&pool, "relay-b", 10, Duration::from_secs(30))
        .await
        .expect("захват");
    assert!(
        b.is_empty(),
        "следующее событие того же поста ждёт, пока выйдет предыдущее"
    );
    db_engine::release(&pool, "relay-a", a[0].seq)
        .await
        .expect("отметка");
    let b = db_engine::claim(&pool, "relay-b", 10, Duration::from_secs(30))
        .await
        .expect("захват");
    assert_eq!(b.len(), 1, "теперь очередь агрегата свободна");
    sqlx::query("UPDATE posts SET likes = 42")
        .execute(&pool)
        .await
        .expect("порча счётчика");
    assert_eq!(
        posts.reconcile_likes().await.expect("сверка"),
        1,
        "сверка поправила один пост"
    );
    assert_eq!(posts.get(first.id).await.map(|post| post.likes), Some(1));
    let _ = drop_schema(&config).await;
}
