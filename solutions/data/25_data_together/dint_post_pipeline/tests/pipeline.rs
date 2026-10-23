use std::collections::HashSet;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use post_pipeline::{Event, Pipeline, Sink};
use scylla::{Session, SessionBuilder};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

struct Env {
    pg: PgPool,
    admin: PgPool,
    scylla: Arc<Session>,
    name: String,
}

impl Env {
    async fn new() -> Env {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let name = format!("pipe_{}_{nanos:x}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
        let url = std::env::var("RUSTLINGS_PG_URL").unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string());
        let base = PgConnectOptions::from_str(&url).expect("адрес Postgres");
        let admin = PgPoolOptions::new().max_connections(1).connect_with(base.clone()).await.expect("Postgres курса не отвечает, запусти docker compose up -d");
        sqlx::query(&format!("CREATE SCHEMA {name}")).execute(&admin).await.expect("схема теста");
        let pg = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(base.options([("search_path", name.as_str())]))
            .await
            .expect("пул на схему теста");
        let node = std::env::var("RUSTLINGS_SCYLLA_URI").unwrap_or_else(|_| "127.0.0.1:59042".to_string());
        let session = SessionBuilder::new().known_node(&node).build().await.expect("Scylla курса не отвечает, запусти docker compose up -d");
        session
            .query(format!("CREATE KEYSPACE {name} WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}"), ())
            .await
            .expect("keyspace теста");
        Env { pg, admin, scylla: Arc::new(session), name }
    }

    async fn pipeline(&self) -> Pipeline {
        Pipeline::new(self.pg.clone(), Arc::clone(&self.scylla), &self.name).await.expect("создание конвейера")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let (pg, admin, scylla, name) = (self.pg.clone(), self.admin.clone(), Arc::clone(&self.scylla), self.name.clone());
        let _ = std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                rt.block_on(async {
                    pg.close().await;
                    let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {name} CASCADE")).execute(&admin).await;
                    let _ = scylla.query(format!("DROP KEYSPACE IF EXISTS {name}"), ()).await;
                });
            }
        })
        .join();
    }
}

#[derive(Default)]
struct Collect {
    events: Mutex<Vec<Event>>,
    accept: Mutex<Option<usize>>,
}

impl Sink for Collect {
    fn publish(&self, event: &Event) -> Result<(), String> {
        let mut accept = self.accept.lock().unwrap();
        if *accept == Some(0) {
            return Err("брокер упал".to_string());
        }
        if let Some(n) = accept.as_mut() {
            *n -= 1;
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

async fn drain(p: &Pipeline, sink: &Collect) {
    while p.relay_once(sink, 10).await.expect("relay") > 0 {}
}

async fn consume_all(p: &Pipeline, events: &[Event]) {
    for e in events {
        p.consume(e).await.expect("потребитель");
    }
}

#[tokio::test]
async fn posts_reach_followers_feeds() {
    let env = Env::new().await;
    let p = env.pipeline().await;
    p.follow(9, 7).await.expect("подписка");
    p.follow(12, 7).await.expect("подписка");
    let a = p.create_post(7, "первый").await.expect("пост");
    let b = p.create_post(7, "второй").await.expect("пост");
    let c = p.create_post(12, "чужой").await.expect("пост");
    let sink = Collect::default();
    drain(&p, &sink).await;
    let events = sink.events.lock().unwrap().clone();
    assert_eq!(events.len(), 3, "три события о трёх постах");
    consume_all(&p, &events).await;
    assert_eq!(p.feed(9).await.expect("лента"), [(b, "второй".to_string()), (a, "первый".to_string())], "подписчик видит посты автора, свежие первыми");
    assert_eq!(p.feed(7).await.expect("лента").len(), 2, "автор видит свои посты");
    assert_eq!(p.feed(12).await.expect("лента"), [(c, "чужой".to_string()), (b, "второй".to_string()), (a, "первый".to_string())]);
    assert!(p.feed(99).await.expect("лента").is_empty(), "у незнакомца пусто");
}

#[tokio::test]
async fn failed_post_leaves_no_event() {
    let env = Env::new().await;
    let p = env.pipeline().await;
    assert!(p.create_post(7, "").await.is_err(), "пустой пост отвергнут");
    let sink = Collect::default();
    drain(&p, &sink).await;
    assert!(sink.events.lock().unwrap().is_empty(), "о несохранённом посте событий нет");
}

#[tokio::test]
async fn crashes_cause_no_loss_and_no_duplicates() {
    let env = Env::new().await;
    let p = env.pipeline().await;
    p.follow(9, 7).await.expect("подписка");
    for i in 0..5 {
        p.create_post(7, &format!("пост {i}")).await.expect("пост");
    }
    let sink = Collect::default();
    *sink.accept.lock().unwrap() = Some(2);
    assert_eq!(p.relay_once(&sink, 10).await.expect("relay"), 2, "брокер упал после двух событий");
    *sink.accept.lock().unwrap() = None;
    sqlx::query("UPDATE outbox SET sent_at = NULL WHERE id IN (SELECT id FROM outbox WHERE sent_at IS NOT NULL ORDER BY id LIMIT 1)")
        .execute(&env.pg)
        .await
        .expect("relay упал после публикации, но до отметки");
    drain(&p, &sink).await;
    let events = sink.events.lock().unwrap().clone();
    let unique: HashSet<i64> = events.iter().map(|e| e.id).collect();
    assert_eq!(unique.len(), 5, "все пять событий дошли");
    assert!(events.len() > 5, "одно событие пришло повторно, как бывает после падения relay");
    consume_all(&p, &events).await;
    consume_all(&p, &events).await;
    assert_eq!(p.feed(9).await.expect("лента").len(), 5, "повторы не размножили ленту");
    let inbox: i64 = sqlx::query_scalar("SELECT count(*) FROM inbox").fetch_one(&env.pg).await.expect("inbox");
    assert_eq!(inbox, 5, "каждое событие обработано один раз");
}

#[tokio::test]
async fn delete_wins_regardless_of_delivery_order() {
    let env = Env::new().await;
    let p = env.pipeline().await;
    p.follow(9, 7).await.expect("подписка");
    let post = p.create_post(7, "удалю").await.expect("пост");
    let keep = p.create_post(7, "оставлю").await.expect("пост");
    p.delete_post(post).await.expect("удаление");
    let sink = Collect::default();
    drain(&p, &sink).await;
    let mut events = sink.events.lock().unwrap().clone();
    assert_eq!(events.len(), 3);
    events.reverse();
    consume_all(&p, &events).await;
    assert_eq!(p.feed(9).await.expect("лента"), [(keep, "оставлю".to_string())], "удаление пришло раньше создания, но пост не воскрес");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_relays_publish_each_event_once() {
    let env = Env::new().await;
    let p = Arc::new(env.pipeline().await);
    for i in 0..30 {
        p.create_post(100 + i % 5, &format!("пост {i}")).await.expect("пост");
    }
    let sink = Arc::new(Collect::default());
    let mut relays = Vec::new();
    for _ in 0..3 {
        let (p, sink) = (Arc::clone(&p), Arc::clone(&sink));
        relays.push(tokio::spawn(async move {
            let mut idle = 0;
            while idle < 3 {
                match p.relay_once(sink.as_ref(), 4).await {
                    Ok(0) => {
                        idle += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    Ok(_) => idle = 0,
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok(())
        }));
    }
    for r in relays {
        r.await.expect("задача").expect("relay");
    }
    let events = sink.events.lock().unwrap().clone();
    assert_eq!(events.len(), 30, "каждое событие опубликовано один раз");
    for author in 100..105 {
        let ids: Vec<i64> = events.iter().filter(|e| e.payload["author"] == author).map(|e| e.aggregate_id).collect();
        assert_eq!(ids.len(), 6);
    }
}
