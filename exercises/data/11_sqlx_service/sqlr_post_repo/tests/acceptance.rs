use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{Connection, PgConnection, PgPool};
use sqlx_post_repo::{Cursor, LikeOutcome, LikeRepository, PgLikeRepository, PgPostRepository, PostRepository, RepoError, apply_schema};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL").unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

struct Env {
    pool: PgPool,
    schema: String,
}

impl Env {
    async fn new() -> Env {
        Env::with(8, false).await
    }

    async fn with(max: u32, serializable: bool) -> Env {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let schema = format!("post_{}_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed), nanos);
        let mut conn = PgConnection::connect(&pg_url())
            .await
            .unwrap_or_else(|e| panic!("Postgres курса не отвечает ({}): {e}. Запусти docker compose up -d", pg_url()));
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\"")).execute(&mut conn).await.unwrap();
        let env = Env { pool: connect(&pg_url(), &schema, max, serializable).await, schema };
        apply_schema(&env.pool).await.expect("схема из schema.sql должна применяться");
        env
    }

    async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap()
    }

    async fn user(&self, name: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO users (username) VALUES ($1) RETURNING id").bind(name).fetch_one(&self.pool).await.unwrap()
    }

    async fn post_at(&self, author: i64, body: &str, ago_minutes: i64) -> i64 {
        sqlx::query_scalar("INSERT INTO posts (author_id, body, created_at) VALUES ($1, $2, date_trunc('minute', now()) - make_interval(mins => $3::int)) RETURNING id")
            .bind(author)
            .bind(body)
            .bind(ago_minutes as i32)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn counted(&self, max: u32) -> (PgPool, Arc<AtomicUsize>) {
        let base = PgConnectOptions::from_str(&pg_url()).unwrap();
        let target = (base.get_host().to_string(), base.get_port());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let seen = seen.clone();
                let target = target.clone();
                tokio::spawn(async move {
                    let Ok(server) = TcpStream::connect((target.0.as_str(), target.1)).await else { return };
                    let (mut cr, mut cw) = client.into_split();
                    let (mut sr, mut sw) = server.into_split();
                    let down = tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut sr, &mut cw).await;
                        let _ = cw.shutdown().await;
                    });
                    let mut started = false;
                    let mut buf = vec![0u8; 16 * 1024];
                    let mut pending: Vec<u8> = Vec::new();
                    while let Ok(n) = cr.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        pending.extend_from_slice(&buf[..n]);
                        loop {
                            if !started {
                                if pending.len() < 4 {
                                    break;
                                }
                                let len = u32::from_be_bytes([pending[0], pending[1], pending[2], pending[3]]) as usize;
                                if pending.len() < len {
                                    break;
                                }
                                pending.drain(..len);
                                started = true;
                                continue;
                            }
                            if pending.len() < 5 {
                                break;
                            }
                            let len = u32::from_be_bytes([pending[1], pending[2], pending[3], pending[4]]) as usize;
                            if pending.len() < 1 + len {
                                break;
                            }
                            if pending[0] == b'B' || pending[0] == b'Q' {
                                seen.fetch_add(1, Ordering::SeqCst);
                            }
                            pending.drain(..1 + len);
                        }
                        if sw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    let _ = sw.shutdown().await;
                    down.abort();
                });
            }
        });
        let options = options(&pg_url(), &self.schema, false).host("127.0.0.1").port(port).ssl_mode(PgSslMode::Disable);
        let pool = PgPoolOptions::new().max_connections(max).acquire_timeout(Duration::from_secs(8)).connect_with(options).await.unwrap();
        count.store(0, Ordering::SeqCst);
        (pool, count)
    }
}

fn options(url: &str, schema: &str, serializable: bool) -> PgConnectOptions {
    let mut pairs = vec![("search_path", schema.to_string()), ("timezone", "UTC".to_string())];
    if serializable {
        pairs.push(("default_transaction_isolation", "serializable".to_string()));
    }
    PgConnectOptions::from_str(url).unwrap().options(pairs)
}

async fn connect(url: &str, schema: &str, max: u32, serializable: bool) -> PgPool {
    PgPoolOptions::new()
        .max_connections(max)
        .acquire_timeout(Duration::from_secs(8))
        .connect_with(options(url, schema, serializable))
        .await
        .unwrap()
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

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn create_returns_the_post_and_links_tags() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let repo = PgPostRepository::new(env.pool.clone());
    let post = repo.create_with_tags(anna, "про Rust".to_string(), names(&["rust", "life"])).await.expect("создание поста с тегами");
    assert_eq!((post.author_id, post.body.as_str(), post.likes_count), (anna, "про Rust", 0), "поле поста должны быть заполнены как в базе");
    assert!(post.id > 0, "id проставляет база");
    assert_eq!(env.scalar(&format!("SELECT count(*) FROM post_tags WHERE post_id = {}", post.id)).await, 2, "к посту должны быть привязаны два тега");
    assert_eq!(env.scalar("SELECT count(*) FROM tags").await, 2, "в таблице тегов должно быть два тега");
}

#[tokio::test]
async fn repeated_and_existing_tags_are_not_duplicated() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    sqlx::query("INSERT INTO tags (name) VALUES ('rust')").execute(&env.pool).await.unwrap();
    let repo = PgPostRepository::new(env.pool.clone());
    let first = repo.create_with_tags(anna, "один".to_string(), names(&["rust", "life", "rust"])).await.expect("первый пост");
    repo.create_with_tags(anna, "два".to_string(), names(&["life", "news"])).await.expect("второй пост");
    assert_eq!(env.scalar("SELECT count(*) FROM tags").await, 3, "всего три разных тега: rust, life, news");
    assert_eq!(env.scalar(&format!("SELECT count(*) FROM post_tags WHERE post_id = {}", first.id)).await, 2, "повтор rust в списке не должен дать вторую привязку");
    let none = repo.create_with_tags(anna, "без тегов".to_string(), vec![]).await.expect("пост без тегов");
    assert_eq!(env.scalar(&format!("SELECT count(*) FROM post_tags WHERE post_id = {}", none.id)).await, 0, "пустой список тегов допустим");
}

#[tokio::test]
async fn failures_are_translated_and_leave_nothing() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let repo = PgPostRepository::new(env.pool.clone());
    let res = repo.create_with_tags(9999, "призрак".to_string(), names(&["rust"])).await;
    assert!(matches!(res, Err(RepoError::NotFound)), "автора нет, ожидалось NotFound");
    let res = repo.create_with_tags(anna, "   ".to_string(), names(&["rust"])).await;
    assert!(matches!(res, Err(RepoError::Invalid)), "пустой текст нарушает CHECK, ожидалось Invalid");
    assert_eq!(env.scalar("SELECT count(*) FROM posts").await, 0, "неудачные попытки не должны оставлять посты");
    assert_eq!(env.scalar("SELECT count(*) FROM tags").await, 0, "и теги: всё создание поста с тегами одна транзакция");
}

#[tokio::test]
async fn nothing_hangs_on_a_pool_of_one_connection() {
    let env = Env::with(1, false).await;
    let anna = env.user("anna").await;
    let repo = PgPostRepository::new(env.pool.clone());
    let likes = PgLikeRepository::new(env.pool.clone());
    let work = async {
        let post = repo.create_with_tags(anna, "один коннект".to_string(), names(&["rust"])).await.expect("создание");
        likes.like(post.id, anna).await.expect("лайк");
        repo.feed(10, None).await.expect("лента");
    };
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .expect("на пуле из одного соединения всё зависло: что-то внутри транзакции просит у пула второе соединение");
}

#[tokio::test]
async fn tag_statements_do_not_depend_on_the_number_of_tags() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let (pool, count) = env.counted(2).await;
    let repo = PgPostRepository::new(pool);
    repo.create_with_tags(anna, "мало".to_string(), names(&["a", "b"])).await.expect("два тега");
    let few = count.swap(0, Ordering::SeqCst);
    let many: Vec<String> = (0..25).map(|i| format!("tag{i}")).collect();
    repo.create_with_tags(anna, "много".to_string(), many).await.expect("25 тегов");
    let lots = count.load(Ordering::SeqCst);
    assert_eq!(few, lots, "число команд должно быть одним и тем же для 2 и 25 тегов, а вышло {few} и {lots}: теги заводятся и привязываются по одному");
    assert!(lots <= 6, "создание поста с тегами выполнило {lots} команд, ожидалось не больше шести (BEGIN, пост, теги, связи, COMMIT)");
}

#[tokio::test]
async fn feed_is_ordered_and_hydrated() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let boris = env.user("boris").await;
    let old = env.post_at(anna, "старый", 30).await;
    let new = env.post_at(boris, "новый", 5).await;
    let tag_rust: i64 = sqlx::query_scalar("INSERT INTO tags (name) VALUES ('rust') RETURNING id").fetch_one(&env.pool).await.unwrap();
    let tag_life: i64 = sqlx::query_scalar("INSERT INTO tags (name) VALUES ('life') RETURNING id").fetch_one(&env.pool).await.unwrap();
    for tag in [tag_rust, tag_life] {
        sqlx::query("INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)").bind(new).bind(tag).execute(&env.pool).await.unwrap();
    }
    let repo = PgPostRepository::new(env.pool.clone());
    let feed = repo.feed(10, None).await.expect("лента");
    let ids: Vec<i64> = feed.iter().map(|i| i.post.id).collect();
    assert_eq!(ids, vec![new, old], "новые посты идут первыми");
    assert_eq!((feed[0].author.as_str(), feed[1].author.as_str()), ("boris", "anna"), "у каждого поста должен быть автор");
    assert_eq!(feed[0].tags, vec!["life", "rust"], "теги поста идут по алфавиту");
    assert!(feed[1].tags.is_empty(), "у старого поста тегов нет");
    assert_eq!(repo.feed(1, None).await.expect("лента с лимитом").len(), 1, "limit ограничивает ленту");
}

#[tokio::test]
async fn cursor_pages_have_no_gaps_and_no_repeats() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    for (i, ago) in [60, 40, 40, 40, 40, 20, 10].into_iter().enumerate() {
        env.post_at(anna, &format!("пост {i}"), ago).await;
    }
    let repo = PgPostRepository::new(env.pool.clone());
    let mut all: Vec<i64> = Vec::new();
    let mut after: Option<Cursor> = None;
    for _ in 0..10 {
        let page = repo.feed(3, after).await.expect("страница ленты");
        if page.is_empty() {
            break;
        }
        all.extend(page.iter().map(|i| i.post.id));
        let last = &page.last().unwrap().post;
        let created: DateTime<Utc> = last.created_at;
        after = Some(Cursor { created_at: created, id: last.id });
    }
    let expected: Vec<i64> = sqlx::query_scalar("SELECT id FROM posts ORDER BY created_at DESC, id DESC").fetch_all(&env.pool).await.unwrap();
    assert_eq!(all, expected, "страницы по курсору вместе должны дать всю ленту один раз, в том же порядке (четыре поста с одинаковым временем не должны теряться)");
}

#[tokio::test]
async fn feed_statements_do_not_depend_on_the_page_size() {
    let env = Env::new().await;
    let users = [env.user("anna").await, env.user("boris").await, env.user("clara").await];
    for i in 0..25 {
        env.post_at(users[i % 3], &format!("пост {i}"), i as i64).await;
    }
    let (pool, count) = env.counted(2).await;
    let repo = PgPostRepository::new(pool);
    let small = repo.feed(3, None).await.expect("маленькая страница");
    let few = count.swap(0, Ordering::SeqCst);
    let big = repo.feed(25, None).await.expect("большая страница");
    let lots = count.load(Ordering::SeqCst);
    assert_eq!((small.len(), big.len()), (3, 25), "размер страниц");
    assert!(few <= 3 && lots <= 3, "лента выполняет не больше трёх команд (посты, авторы, теги) вне зависимости от размера, а вышло {few} и {lots}: на каждый пост ходить в базу нельзя");
}

#[tokio::test]
async fn like_and_unlike_keep_the_counter_exact() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let boris = env.user("boris").await;
    let post = env.post_at(anna, "лайкни меня", 1).await;
    let likes = PgLikeRepository::new(env.pool.clone());
    assert_eq!(likes.like(post, boris).await.expect("лайк"), LikeOutcome::Added, "первый лайк новый");
    assert_eq!(likes.like(post, boris).await.expect("повторный лайк"), LikeOutcome::AlreadyLiked, "повторный лайк не считается");
    assert_eq!(likes.like(post, anna).await.expect("лайк автора"), LikeOutcome::Added, "второй пользователь");
    assert_eq!(env.scalar(&format!("SELECT likes_count::bigint FROM posts WHERE id = {post}")).await, 2, "счётчик равен числу разных лайков");
    assert!(likes.unlike(post, boris).await.expect("снять лайк"), "лайк был, снят");
    assert!(!likes.unlike(post, boris).await.expect("повторное снятие"), "второй раз снимать нечего");
    assert_eq!(env.scalar(&format!("SELECT likes_count::bigint FROM posts WHERE id = {post}")).await, 1, "счётчик после снятия");
    assert_eq!(env.scalar(&format!("SELECT count(*) FROM likes WHERE post_id = {post}")).await, 1, "в таблице лайков осталась одна строка");
}

#[tokio::test]
async fn unknown_post_or_user_is_not_found() {
    let env = Env::new().await;
    let anna = env.user("anna").await;
    let post = env.post_at(anna, "есть", 1).await;
    let likes = PgLikeRepository::new(env.pool.clone());
    assert!(matches!(likes.like(9999, anna).await, Err(RepoError::NotFound)), "поста нет, ожидалось NotFound");
    assert!(matches!(likes.like(post, 9999).await, Err(RepoError::NotFound)), "пользователя нет, ожидалось NotFound");
    assert_eq!(env.scalar(&format!("SELECT likes_count::bigint FROM posts WHERE id = {post}")).await, 0, "ошибочные лайки не должны трогать счётчик");
}

#[tokio::test]
async fn concurrent_likes_survive_serialization_failures() {
    let env = Env::with(8, true).await;
    let anna = env.user("anna").await;
    let post = env.post_at(anna, "популярный", 1).await;
    let mut voters = Vec::new();
    for i in 0..10 {
        voters.push(env.user(&format!("fan{i}")).await);
    }
    let likes = Arc::new(PgLikeRepository::new(env.pool.clone()));
    let mut tasks = Vec::new();
    for voter in voters {
        let likes = likes.clone();
        tasks.push(tokio::spawn(async move { likes.like(post, voter).await }));
    }
    let mut added = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(LikeOutcome::Added) => added += 1,
            Ok(LikeOutcome::AlreadyLiked) => panic!("у каждого фаната свой лайк, повторных быть не должно"),
            Err(RepoError::Db(e)) => panic!("база по умолчанию работает в SERIALIZABLE и отвечает 40001 при конфликтах, репозиторий обязан повторять транзакцию. Ошибка: {e}"),
            Err(e) => panic!("неожиданная ошибка: {e:?}"),
        }
    }
    assert_eq!(added, 10, "все десять лайков должны пройти");
    assert_eq!(env.scalar(&format!("SELECT likes_count::bigint FROM posts WHERE id = {post}")).await, 10, "счётчик должен показывать 10");
    assert_eq!(env.scalar(&format!("SELECT count(*) FROM likes WHERE post_id = {post}")).await, 10, "в таблице лайков должно быть 10 строк");
}

#[tokio::test]
async fn the_same_user_racing_counts_once() {
    let env = Env::with(8, true).await;
    let anna = env.user("anna").await;
    let post = env.post_at(anna, "гонка", 1).await;
    let likes = Arc::new(PgLikeRepository::new(env.pool.clone()));
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let likes = likes.clone();
        tasks.push(tokio::spawn(async move { likes.like(post, anna).await }));
    }
    let mut added = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(LikeOutcome::Added) => added += 1,
            Ok(LikeOutcome::AlreadyLiked) => {}
            Err(e) => panic!("гонка одинаковых лайков не должна давать ошибок: {e:?}"),
        }
    }
    assert_eq!(added, 1, "из десяти одинаковых лайков засчитан ровно один");
    assert_eq!(env.scalar(&format!("SELECT likes_count::bigint FROM posts WHERE id = {post}")).await, 1, "счётчик равен единице");
}

#[test]
fn no_string_built_sql() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/postgres.rs")).expect("не удалось прочитать src/postgres.rs");
    assert!(!src.contains("format!("), "SQL нельзя собирать форматированием: константы с $n и bind");
}
