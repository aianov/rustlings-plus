use posts_service::{map_upstream, run};
use proto::posts::v1::posts_client::PostsClient;
use proto::posts::v1::{CreatePostRequest, GetPostRequest};
use proto::users::v1::users_server::{Users, UsersServer};
use proto::users::v1::{CreateUserRequest, GetUserRequest, User};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

type Script = Box<dyn Fn(u64) -> Result<User, Status> + Send + Sync>;

struct Inner {
    script: Script,
    delay: Duration,
    calls: AtomicUsize,
    request_ids: Mutex<Vec<Option<String>>>,
    accepted: AtomicUsize,
}

#[derive(Clone)]
struct Fake(Arc<Inner>);

#[tonic::async_trait]
impl Users for Fake {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        let rid = request.metadata().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_string);
        self.0.request_ids.lock().unwrap().push(rid);
        tokio::time::sleep(self.0.delay).await;
        (self.0.script)(request.into_inner().id).map(Response::new)
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

fn user(id: u64) -> User {
    User { id, handle: format!("u{id}"), display_name: String::new() }
}

async fn fake_users(delay: Duration, script: Script) -> (Channel, Arc<Inner>) {
    let inner = Arc::new(Inner { script, delay, calls: AtomicUsize::new(0), request_ids: Mutex::new(Vec::new()), accepted: AtomicUsize::new(0) });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let counter = inner.clone();
    let incoming = TcpListenerStream::new(listener).map(move |c| {
        if c.is_ok() {
            counter.accepted.fetch_add(1, Ordering::SeqCst);
        }
        c
    });
    tokio::spawn(Server::builder().add_service(UsersServer::new(Fake(inner.clone()))).serve_with_incoming(incoming));
    (lazy(addr), inner)
}

fn lazy(addr: SocketAddr) -> Channel {
    Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()
}

async fn posts_client(users: Channel) -> PostsClient<Channel> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(run(listener, users));
    PostsClient::new(lazy(addr))
}

fn create(author_id: u64, text: &str) -> CreatePostRequest {
    CreatePostRequest { author_id, text: text.to_string() }
}

#[tokio::test]
async fn whole_scenario_with_the_real_users_service() {
    let users = users_service::start().await.unwrap();
    let mut posts = posts_client(lazy(users)).await;
    let post = within("create", posts.create_post(create(2, "  привет  "))).await.unwrap().into_inner();
    assert_eq!((post.id, post.text.as_str()), (1, "привет"), "текст обрезается, первый пост получает номер 1");
    assert_eq!(post.author.as_ref().map(|a| a.handle.as_str()), Some("bob"), "автор берётся из ответа users");
    let again = within("get", posts.get_post(GetPostRequest { id: 1 })).await.unwrap().into_inner();
    assert_eq!(again, post);
    let missing = within("get", posts.get_post(GetPostRequest { id: 9 })).await.unwrap_err();
    assert_eq!(missing.code(), Code::NotFound);
    let nobody = within("create", posts.create_post(create(42, "hi"))).await.unwrap_err();
    assert_eq!(nobody.code(), Code::InvalidArgument, "автора 42 нет: клиент передал плохой author_id, а не наткнулся на NotFound");
    assert!(nobody.message().contains("author_id"), "в сообщении должно быть имя поля: {}", nobody.message());
}

#[tokio::test]
async fn upstream_codes_are_translated_by_the_table() {
    let table = [
        (Code::NotFound, Code::InvalidArgument),
        (Code::Unavailable, Code::Unavailable),
        (Code::ResourceExhausted, Code::Unavailable),
        (Code::DeadlineExceeded, Code::DeadlineExceeded),
        (Code::Cancelled, Code::DeadlineExceeded),
        (Code::Internal, Code::Internal),
        (Code::PermissionDenied, Code::Internal),
        (Code::Unknown, Code::Internal),
    ];
    for (from, expected) in table {
        let (users, _inner) = fake_users(Duration::ZERO, Box::new(move |_| Err(Status::new(from, "секретный текст users: password=hunter2")))).await;
        let mut posts = posts_client(users).await;
        let err = within("create", posts.create_post(create(1, "hi"))).await.unwrap_err();
        assert_eq!(err.code(), expected, "users ответил {from:?}, а клиенту ушло {:?}, ждали {expected:?}", err.code());
        assert!(!err.message().contains("hunter2") && !err.message().contains("секретный"), "текст чужой ошибки просочился к клиенту: {}", err.message());
    }
}

#[test]
fn map_upstream_is_a_plain_function() {
    assert_eq!(map_upstream(Status::not_found("x")).code(), Code::InvalidArgument);
    assert_eq!(map_upstream(Status::data_loss("x")).code(), Code::Internal);
}

#[tokio::test]
async fn empty_text_is_rejected_without_calling_users() {
    let (users, inner) = fake_users(Duration::ZERO, Box::new(|id| Ok(user(id)))).await;
    let mut posts = posts_client(users).await;
    for blank in ["", "   ", "\n\t"] {
        let err = within("create", posts.create_post(create(1, blank))).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
        assert!(err.message().contains("text"), "в сообщении должно быть имя поля text: {}", err.message());
    }
    assert_eq!(inner.calls.load(Ordering::SeqCst), 0, "пустой текст виден сразу, ходить за автором незачем");
}

#[tokio::test]
async fn request_id_travels_to_users() {
    let (users, inner) = fake_users(Duration::ZERO, Box::new(|id| Ok(user(id)))).await;
    let mut posts = posts_client(users).await;
    let mut with_id = Request::new(create(1, "hi"));
    with_id.metadata_mut().insert("x-request-id", MetadataValue::from_static("req-77"));
    within("create", posts.create_post(with_id)).await.unwrap();
    within("create", posts.create_post(create(1, "again"))).await.unwrap();
    let seen = inner.request_ids.lock().unwrap().clone();
    assert_eq!(seen, vec![Some("req-77".to_string()), None], "x-request-id должен доезжать до users, а без него вызов идёт без заголовка");
}

#[tokio::test]
async fn twenty_creates_share_one_connection_and_run_in_parallel() {
    let (users, inner) = fake_users(Duration::from_millis(200), Box::new(|id| Ok(user(id)))).await;
    let posts = posts_client(users).await;
    let started = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..20 {
        let mut posts = posts.clone();
        tasks.push(tokio::spawn(async move { posts.create_post(create(1, &format!("пост {i}"))).await.unwrap().into_inner().id }));
    }
    let mut ids = HashSet::new();
    for task in tasks {
        ids.insert(within("create", task).await.unwrap());
    }
    let took = started.elapsed();
    assert_eq!(ids.len(), 20, "номера постов повторились: номер и запись должны идти под одним замком");
    assert_eq!(inner.accepted.load(Ordering::SeqCst), 1, "posts открыл больше одного соединения к users: канал создаётся один раз");
    assert!(took < Duration::from_millis(1500), "двадцать вызовов по 200 мс заняли {took:?}: обработчик держит замок или клиента через await");
}
