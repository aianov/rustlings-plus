use posts_service::{run, Config, LogLine, Sink, SERVICE};
use proto::feed::v1::feed_client::FeedClient;
use proto::feed::v1::{Item, PublishRequest, TimelineRequest};
use proto::users::v1::users_server::{Users, UsersServer};
use proto::users::v1::{CreateUserRequest, GetUserRequest, User};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status, Streaming};
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus;

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

type Seen = Vec<(u64, Option<Duration>, Option<String>)>;

struct Spy {
    delay: Duration,
    fail: Option<Code>,
    calls: AtomicUsize,
    seen: Mutex<Seen>,
    accepted: AtomicUsize,
}

#[derive(Clone)]
struct FakeUsers(Arc<Spy>);

fn parse_timeout(v: &str) -> Option<Duration> {
    let (digits, unit) = v.split_at(v.len() - 1);
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        "H" => Duration::from_secs(n * 3600),
        "M" => Duration::from_secs(n * 60),
        "S" => Duration::from_secs(n),
        "m" => Duration::from_millis(n),
        "u" => Duration::from_micros(n),
        "n" => Duration::from_nanos(n),
        _ => return None,
    })
}

#[tonic::async_trait]
impl Users for FakeUsers {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        let timeout = request.metadata().get("grpc-timeout").and_then(|v| v.to_str().ok()).and_then(parse_timeout);
        let rid = request.metadata().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_string);
        let id = request.into_inner().id;
        self.0.seen.lock().unwrap().push((id, timeout, rid));
        tokio::time::sleep(self.0.delay).await;
        if let Some(code) = self.0.fail {
            return Err(Status::new(code, "секретный текст users: password=hunter2"));
        }
        let handle = if id == 1 { "ann" } else { "bob" };
        Ok(Response::new(User { id, handle: handle.to_string(), display_name: handle.to_uppercase() }))
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

struct Env {
    feed: Channel,
    spy: Arc<Spy>,
    sink: Sink,
    stop: Option<oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

async fn env(delay: Duration, fail: Option<Code>, grace: Duration) -> Env {
    let spy = Arc::new(Spy { delay, fail, calls: AtomicUsize::new(0), seen: Mutex::new(Vec::new()), accepted: AtomicUsize::new(0) });
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let users_addr = l.local_addr().unwrap();
    let counter = spy.clone();
    let incoming = TcpListenerStream::new(l).map(move |c| {
        if c.is_ok() {
            counter.accepted.fetch_add(1, Ordering::SeqCst);
        }
        c
    });
    tokio::spawn(Server::builder().add_service(UsersServer::new(FakeUsers(spy.clone()))).serve_with_incoming(incoming));
    let users: Channel = Endpoint::from_shared(format!("http://{users_addr}")).unwrap().connect_lazy();
    let sink: Sink = Arc::default();
    let config = Config { tokens: vec![("t-ann".to_string(), 1), ("t-bob".to_string(), 2)], grace, sink: sink.clone() };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let (stop, rx) = oneshot::channel();
    let server = tokio::spawn(run(listener, users, config, rx));
    let feed = Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
    Env { feed, spy, sink, stop: Some(stop), server }
}

fn authed<T>(message: T, token: &str) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert("authorization", MetadataValue::try_from(format!("Bearer {token}")).unwrap());
    request
}

async fn publish(client: &mut FeedClient<Channel>, token: &str, text: &str) -> u64 {
    within("publish", client.publish(authed(PublishRequest { text: text.to_string() }, token))).await.unwrap().into_inner().id
}

async fn drain(mut stream: Streaming<Item>) -> (Vec<Item>, Option<Status>) {
    let mut items = Vec::new();
    loop {
        match within("чтение ленты", stream.message()).await {
            Ok(Some(item)) => items.push(item),
            Ok(None) => return (items, None),
            Err(status) => return (items, Some(status)),
        }
    }
}

#[tokio::test]
async fn token_is_required_but_health_is_open() {
    let e = env(ms(0), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    let err = within("publish", client.publish(PublishRequest { text: "hi".into() })).await.unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated, "без токена вызов должен отклоняться");
    let err = within("publish", client.publish(authed(PublishRequest { text: "hi".into() }, "чужой"))).await.unwrap_err();
    assert_eq!(err.code(), Code::Unauthenticated);
    let mut health = HealthClient::new(e.feed.clone());
    let status = within("health", health.check(HealthCheckRequest { service: SERVICE.to_string() })).await.expect("проверка здоровья должна быть открыта без токена").into_inner().status();
    assert_eq!(status, ServingStatus::Serving);
}

#[tokio::test]
async fn timeline_is_newest_first_with_authors_and_limits() {
    let e = env(ms(0), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    assert_eq!(publish(&mut client, "t-ann", "  первый  ").await, 1);
    assert_eq!(publish(&mut client, "t-bob", "второй").await, 2);
    assert_eq!(publish(&mut client, "t-ann", "третий").await, 3);
    let (items, err) = drain(within("timeline", client.timeline(authed(TimelineRequest { limit: 0 }, "t-bob"))).await.unwrap().into_inner()).await;
    assert!(err.is_none());
    let view: Vec<(u64, String, String)> = items.iter().map(|i| (i.id, i.author.as_ref().unwrap().handle.clone(), i.text.clone())).collect();
    assert_eq!(view, vec![(3, "ann".into(), "третий".into()), (2, "bob".into(), "второй".into()), (1, "ann".into(), "первый".into())], "лента: новые первыми, текст обрезан, автор из users");
    let (two, _) = drain(client.timeline(authed(TimelineRequest { limit: 2 }, "t-ann")).await.unwrap().into_inner()).await;
    assert_eq!(two.len(), 2, "limit = 2");
    let err = client.timeline(authed(TimelineRequest { limit: 101 }, "t-ann")).await.unwrap_err();
    assert_eq!(err.code(), Code::InvalidArgument);
    for bad in ["", "   ", &"я".repeat(281)] {
        let err = client.publish(authed(PublishRequest { text: bad.to_string() }, "t-ann")).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument, "текст {bad:?} недопустим");
    }
}

#[tokio::test]
async fn one_users_call_per_distinct_author_over_one_connection() {
    let e = env(ms(0), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    for (token, text) in [("t-ann", "a"), ("t-ann", "b"), ("t-bob", "c"), ("t-ann", "d"), ("t-bob", "e")] {
        publish(&mut client, token, text).await;
    }
    for _ in 0..3 {
        let (items, err) = drain(client.timeline(authed(TimelineRequest { limit: 0 }, "t-ann")).await.unwrap().into_inner()).await;
        assert!(err.is_none() && items.len() == 5);
    }
    assert_eq!(e.spy.calls.load(Ordering::SeqCst), 6, "в каждой ленте по одному вызову users на автора (два автора, три ленты)");
    assert_eq!(e.spy.accepted.load(Ordering::SeqCst), 1, "канал к users должен быть один на все вызовы");
}

#[tokio::test]
async fn the_budget_is_shared_along_the_timeline() {
    let e = env(ms(150), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    publish(&mut client, "t-ann", "a").await;
    publish(&mut client, "t-bob", "b").await;
    let mut request = authed(TimelineRequest { limit: 0 }, "t-ann");
    request.set_timeout(ms(400));
    let (items, err) = drain(within("timeline", client.timeline(request)).await.unwrap().into_inner()).await;
    assert!(err.is_none() && items.len() == 2, "бюджета 400 мс хватает на два вызова по 150 мс");
    let seen = e.spy.seen.lock().unwrap().clone();
    let first = seen[0].1.expect("первый вызов users без срока");
    let second = seen[1].1.expect("второй вызов users без срока");
    assert!(first <= ms(400) && first >= ms(300), "первому вызову users достаётся почти весь бюджет, получил {first:?}");
    assert!(second + ms(100) <= first, "второй вызов получил {second:?} при первом {first:?}: срок должен таять по мере работы ленты");
}

#[tokio::test]
async fn out_of_budget_is_an_error_inside_the_stream() {
    let e = env(ms(150), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    publish(&mut client, "t-ann", "a").await;
    publish(&mut client, "t-bob", "b").await;
    let mut request = authed(TimelineRequest { limit: 0 }, "t-ann");
    request.set_timeout(ms(220));
    let started = Instant::now();
    let (items, err) = drain(within("timeline", client.timeline(request)).await.unwrap().into_inner()).await;
    assert_eq!(items.len(), 1, "первый автор успевает (150 мс из 220), второй нет");
    let err = err.expect("лента оборвана по бюджету, а клиент увидел нормальный конец потока");
    assert_eq!(err.code(), Code::DeadlineExceeded);
    assert!(!err.message().contains("hunter2"));
    assert!(started.elapsed() < ms(600), "лента тянулась {:?} при бюджете 220 мс", started.elapsed());
}

#[tokio::test]
async fn users_errors_arrive_translated_and_without_foreign_text() {
    let e = env(ms(0), Some(Code::PermissionDenied), ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    publish(&mut client, "t-ann", "a").await;
    let (items, err) = drain(client.timeline(authed(TimelineRequest { limit: 0 }, "t-ann")).await.unwrap().into_inner()).await;
    assert!(items.is_empty());
    let err = err.expect("ошибка users должна дойти как Err в потоке");
    assert_eq!(err.code(), Code::Internal, "PermissionDenied от users для нашего клиента это Internal");
    assert!(!err.message().contains("hunter2") && !err.message().contains("секретный"), "чужой текст просочился: {}", err.message());
}

#[tokio::test]
async fn request_id_reaches_users_and_the_log() {
    let e = env(ms(0), None, ms(50)).await;
    let mut client = FeedClient::new(e.feed.clone());
    publish(&mut client, "t-ann", "a").await;
    let mut request = authed(TimelineRequest { limit: 0 }, "t-ann");
    request.metadata_mut().insert("x-request-id", MetadataValue::from_static("req-42"));
    drain(client.timeline(request).await.unwrap().into_inner()).await;
    assert_eq!(e.spy.seen.lock().unwrap()[0].2.as_deref(), Some("req-42"), "x-request-id должен доезжать до users");
    let lines: Vec<LogLine> = e.sink.lock().unwrap().clone();
    let line = lines.iter().find(|l| l.path == "/feed.v1.Feed/Timeline").expect("слой журнала должен видеть вызов ленты");
    assert_eq!(line.request_id, "req-42");
}

#[tokio::test]
async fn shutdown_lets_the_running_timeline_finish() {
    let mut e = env(ms(300), None, ms(300)).await;
    let mut client = FeedClient::new(e.feed.clone());
    publish(&mut client, "t-ann", "a").await;
    let mut ready = false;
    let mut health = HealthClient::new(e.feed.clone());
    for _ in 0..20 {
        if health.check(HealthCheckRequest { service: SERVICE.to_string() }).await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(ms(50)).await;
    }
    assert!(ready);
    let mut running = client.clone();
    let timeline = tokio::spawn(async move {
        let stream = running.timeline(authed(TimelineRequest { limit: 0 }, "t-ann")).await.unwrap().into_inner();
        drain(stream).await
    });
    tokio::time::sleep(ms(100)).await;
    e.stop.take().unwrap().send(()).unwrap();
    tokio::time::sleep(ms(60)).await;
    let during = within("health", health.check(HealthCheckRequest { service: SERVICE.to_string() })).await.expect("в период грации здоровье отвечает").into_inner().status();
    assert_eq!(during, ServingStatus::NotServing, "после сигнала остановки сервис должен перейти в NOT_SERVING");
    let (items, err) = within("лента", timeline).await.unwrap();
    assert!(err.is_none() && items.len() == 1, "идущая лента должна доработать до конца: {err:?}");
    within("остановка", e.server).await.unwrap().expect("остановка должна кончаться Ok");
}
