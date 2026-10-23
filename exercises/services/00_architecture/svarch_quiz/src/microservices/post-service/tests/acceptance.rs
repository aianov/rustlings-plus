use contracts::{PostId, UserId};
use futures::future::{BoxFuture, join_all};
use post_service::bootstrap;
use post_service::common::Post;
use post_service::controller::{Controller, Request, Response};
use post_service::repository::{InMemoryPosts, PostRepository, RepoError};
use post_service::service::{DirectoryError, UserDirectory};
use store_kit::Store;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Barrier;

struct Directory {
    known: Vec<u64>,
    down: bool,
    asked: AtomicUsize,
    gate: Option<Barrier>,
}

impl Directory {
    fn knows(ids: &[u64]) -> Arc<Self> {
        Arc::new(Self { known: ids.to_vec(), down: false, asked: AtomicUsize::new(0), gate: None })
    }

    fn broken() -> Arc<Self> {
        Arc::new(Self { known: vec![], down: true, asked: AtomicUsize::new(0), gate: None })
    }

    fn gated(ids: &[u64], parties: usize) -> Arc<Self> {
        Arc::new(Self {
            known: ids.to_vec(),
            down: false,
            asked: AtomicUsize::new(0),
            gate: Some(Barrier::new(parties)),
        })
    }
}

impl UserDirectory for Directory {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>> {
        Box::pin(async move {
            self.asked.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.gate {
                gate.wait().await;
            }
            if self.down {
                Err(DirectoryError::Unavailable)
            } else {
                Ok(self.known.contains(&id.0))
            }
        })
    }
}

fn request(method: &str, path: &str, body: &str) -> Request {
    Request { method: method.to_string(), path: path.to_string(), body: body.to_string() }
}

fn post_body(author: u64, text: &str) -> String {
    json!({ "author": author, "text": text }).to_string()
}

fn body(resp: &Response) -> Value {
    serde_json::from_str(&resp.body).expect("тело ответа должно быть JSON")
}

async fn call(app: &Controller, method: &str, path: &str, payload: &str) -> Response {
    tokio::time::timeout(Duration::from_secs(10), app.handle(request(method, path, payload)))
        .await
        .expect("запрос не завершился за 10 секунд")
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

fn forbid(layer: &str, needles: &[&str], why: &str) {
    for path in rs_files(&src().join(layer)) {
        let text = fs::read_to_string(&path).unwrap();
        for needle in needles {
            assert!(!text.contains(needle), "в {} найдено `{needle}`: {why}", path.display());
        }
    }
}

#[test]
fn layout_and_arrows() {
    let root = src();
    for file in ["main.rs", "lib.rs"] {
        assert!(root.join(file).is_file(), "нет src/{file}");
    }
    for layer in ["bootstrap", "controller", "dto", "service", "repository", "common"] {
        assert!(root.join(layer).join("mod.rs").is_file(), "нет src/{layer}/mod.rs");
        assert!(!root.join(format!("{layer}.rs")).exists(), "src/{layer}.rs лежит рядом с папкой src/{layer}/");
    }
    let main = fs::read_to_string(root.join("main.rs")).unwrap();
    assert!(main.lines().count() <= 20, "main.rs должен быть тонким: до 20 строк");
    forbid("controller", &["repository::"], "контроллер не ходит в репозиторий");
    forbid(
        "service",
        &["serde_json", "controller::", "dto::", "Request", "Response"],
        "service не знает о транспорте",
    );
    forbid("repository", &["service::", "controller::", "dto::", "serde"], "репозиторий не знает верхних слоёв");
    assert!(root.join("repository/memory.rs").is_file(), "реализация в памяти лежит в src/repository/memory.rs");
    let dto = fs::read_to_string(root.join("dto/mod.rs")).unwrap();
    assert!(dto.contains("CreatePostDto") && dto.contains("PostDto"), "DTO живут в src/dto/mod.rs");
    let common = fs::read_to_string(root.join("common/mod.rs")).unwrap();
    assert!(common.contains("struct Post"), "доменный тип Post живёт в src/common/mod.rs");
}

struct BrokenStorage;

impl PostRepository for BrokenStorage {
    fn create_if_absent(&self, _author: UserId, _text: String) -> BoxFuture<'_, Result<Post, RepoError>> {
        Box::pin(async { Err(RepoError::Unavailable) })
    }

    fn find(&self, _id: PostId) -> BoxFuture<'_, Option<Post>> {
        Box::pin(async { None })
    }

    fn by_author(&self, _author: UserId) -> BoxFuture<'_, Vec<Post>> {
        Box::pin(async { vec![] })
    }
}

#[tokio::test]
async fn storage_failure_is_a_503() {
    let app = bootstrap::build_with(Directory::knows(&[1]), Arc::new(BrokenStorage));
    let resp = call(&app, "POST", "/posts", &post_body(1, "hi")).await;
    assert_eq!(
        (resp.status, body(&resp)["error"].clone()),
        (503, json!("storage_unavailable")),
        "репозиторий сказал Unavailable: клиент получает 503 storage_unavailable"
    );
}

#[tokio::test]
async fn create_and_read_back() {
    let app = bootstrap::build(Directory::knows(&[1, 2]));
    let created = call(&app, "POST", "/posts", &post_body(1, "  привет, мир ")).await;
    assert_eq!(created.status, 201, "создание поста: {}", created.body);
    assert_eq!(
        body(&created),
        json!({"id": 1, "author": 1, "text": "привет, мир"}),
        "тело ответа: id, author, обрезанный text"
    );
    let read = call(&app, "GET", "/posts/1", "").await;
    assert_eq!(read.status, 200, "созданный пост читается");
    assert_eq!(body(&read)["text"], "привет, мир", "текст тот же");
    assert_eq!(call(&app, "GET", "/posts/9", "").await.status, 404, "неизвестный пост это 404");
    assert_eq!(call(&app, "GET", "/posts/x", "").await.status, 400, "битый id это 400");
    assert_eq!(call(&app, "GET", "/unknown", "").await.status, 404, "неизвестный маршрут это 404");
}

#[tokio::test]
async fn validation_errors_have_stable_codes() {
    let app = bootstrap::build(Directory::knows(&[1]));
    let bad = call(&app, "POST", "/posts", "{oops").await;
    assert_eq!((bad.status, body(&bad)["error"].clone()), (400, json!("bad_json")), "битый JSON");
    let empty = call(&app, "POST", "/posts", &post_body(1, "   ")).await;
    assert_eq!((empty.status, body(&empty)["error"].clone()), (422, json!("empty_text")), "пустой текст");
    let long = call(&app, "POST", "/posts", &post_body(1, &"я".repeat(281))).await;
    assert_eq!((long.status, body(&long)["error"].clone()), (422, json!("text_too_long")), "281 символ");
    let edge = call(&app, "POST", "/posts", &post_body(1, &"я".repeat(280))).await;
    assert_eq!(edge.status, 201, "280 символов (не байт) проходят");
}

#[tokio::test]
async fn author_must_exist() {
    let app = bootstrap::build(Directory::knows(&[1]));
    let resp = call(&app, "POST", "/posts", &post_body(2, "hi")).await;
    assert_eq!((resp.status, body(&resp)["error"].clone()), (404, json!("author_not_found")), "автора нет");
    let list = call(&app, "GET", "/users/2/posts", "").await;
    assert_eq!(body(&list), json!([]), "пост от несуществующего автора не сохранён");
}

#[tokio::test]
async fn broken_directory_is_503_not_404() {
    let app = bootstrap::build(Directory::broken());
    let resp = call(&app, "POST", "/posts", &post_body(1, "hi")).await;
    assert_eq!(
        (resp.status, body(&resp)["error"].clone()),
        (503, json!("users_unavailable")),
        "сосед недоступен: это 503, а не «автора нет»"
    );
}

#[tokio::test]
async fn local_checks_come_before_the_network() {
    let directory = Directory::knows(&[1]);
    let app = bootstrap::build(directory.clone());
    call(&app, "POST", "/posts", &post_body(1, "  ")).await;
    call(&app, "POST", "/posts", "{").await;
    assert_eq!(directory.asked.load(Ordering::SeqCst), 0, "до соседа доходят только запросы, прошедшие локальные проверки");
}

#[tokio::test]
async fn posts_of_an_author_in_creation_order() {
    let app = bootstrap::build(Directory::knows(&[1, 2]));
    for (author, text) in [(1, "первый"), (2, "чужой"), (1, "второй")] {
        assert_eq!(call(&app, "POST", "/posts", &post_body(author, text)).await.status, 201, "создание");
    }
    let list = call(&app, "GET", "/users/1/posts", "").await;
    assert_eq!(list.status, 200, "список постов автора");
    let texts: Vec<_> = body(&list).as_array().unwrap().iter().map(|p| p["text"].clone()).collect();
    assert_eq!(texts, vec![json!("первый"), json!("второй")], "только посты автора, в порядке создания");
}

#[tokio::test]
async fn identical_post_is_a_conflict() {
    let app = bootstrap::build(Directory::knows(&[1, 2]));
    assert_eq!(call(&app, "POST", "/posts", &post_body(1, "привет")).await.status, 201, "первый раз");
    let dup = call(&app, "POST", "/posts", &post_body(1, "  привет  ")).await;
    assert_eq!(
        (dup.status, body(&dup)["error"].clone()),
        (409, json!("duplicate_post")),
        "тот же автор и тот же текст после обрезки: 409"
    );
    assert_eq!(call(&app, "POST", "/posts", &post_body(2, "привет")).await.status, 201, "другой автор может");
    let after = call(&app, "POST", "/posts", &post_body(1, "другое")).await;
    assert_eq!(body(&after)["id"], 3, "отказ не тратит id: после постов 1 и 2 следующий получает 3");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_distinct_posts_get_distinct_ids() {
    let app = Arc::new(bootstrap::build(Directory::knows(&[1])));
    let all = join_all((0..32).map(|i| {
        let app = Arc::clone(&app);
        async move { call(&app, "POST", "/posts", &post_body(1, &format!("пост {i}"))).await }
    }))
    .await;
    let mut ids: Vec<u64> = all.iter().map(|r| body(r)["id"].as_u64().expect("id")).collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=32).collect::<Vec<_>>(), "32 параллельных поста получают номера 1..=32 без дыр и повторов");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn concurrent_duplicates_exactly_one_wins() {
    const RACERS: usize = 4;
    for round in 0..5 {
        let armed = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(std::sync::Barrier::new(RACERS));
        let store = {
            let (armed, gate) = (Arc::clone(&armed), Arc::clone(&gate));
            Store::with_hook(Vec::new(), move || {
                if armed.load(Ordering::SeqCst) {
                    tokio::task::block_in_place(|| gate.wait());
                }
            })
        };
        let app = Arc::new(bootstrap::build_with(Directory::gated(&[1], RACERS), Arc::new(InMemoryPosts::new(store))));
        let text = format!("гонка {round}");
        let tasks: Vec<_> = (0..RACERS)
            .map(|_| {
                let app = Arc::clone(&app);
                let text = text.clone();
                tokio::spawn(async move { call(&app, "POST", "/posts", &post_body(1, &text)).await })
            })
            .collect();
        let all: Vec<Response> = join_all(tasks).await.into_iter().map(|r| r.unwrap()).collect();
        armed.store(false, Ordering::SeqCst);
        let created = all.iter().filter(|r| r.status == 201).count();
        let conflicts = all.iter().filter(|r| r.status == 409).count();
        assert_eq!(
            (created, conflicts),
            (1, RACERS - 1),
            "раунд {round}: одинаковые посты одновременно: ждали один 201 и {} раз 409, а пришло {created} и {conflicts}. Проверка дубликата и запись должны быть одной атомарной операцией репозитория",
            RACERS - 1
        );
        let list = call(&app, "GET", "/users/1/posts", "").await;
        assert_eq!(body(&list).as_array().unwrap().len(), 1, "раунд {round}: в хранилище ровно один пост");
    }
}
