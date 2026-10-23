use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use grpc_kit::{
    Access, AccessLog, Caller, LogSink, Readiness, Stamp, USER_ID, Upstream, budget_of, caller,
    caller_of, drain, parse_grpc_timeout,
};
use proto::{REQUEST_ID, domain_code};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::Interceptor;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request};
use tonic_health::ServingStatus;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus as Seen;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::server::{HealthReporter, health_reporter};

const WAIT: Duration = Duration::from_secs(5);

const PROBE: &str = "probe.v1.Probe";

struct Probe;

impl tonic::server::NamedService for Probe {
    const NAME: &'static str = PROBE;
}

fn metadata<'a, T>(request: &'a Request<T>, key: &str) -> Option<&'a str> {
    request.metadata().get(key).and_then(|v| v.to_str().ok())
}

#[test]
fn stamp_puts_request_id_user_and_budget() {
    let mut stamp = Stamp::new("abc123")
        .user(7)
        .within(Duration::from_millis(250));
    let request = stamp.call(Request::new(())).expect("штамп ставится");
    assert_eq!(metadata(&request, REQUEST_ID), Some("abc123"));
    assert_eq!(metadata(&request, USER_ID), Some("7"));
    let budget = budget_of(&request).expect("срок едет в grpc-timeout");
    assert!(
        budget <= Duration::from_millis(250) && budget > Duration::from_millis(150),
        "остаток бюджета, а не весь срок с запасом: {budget:?}"
    );
    let mut bare = Stamp::new("r");
    let request = bare.call(Request::new(())).expect("штамп без пользователя");
    assert_eq!(
        metadata(&request, USER_ID),
        None,
        "анонимный вызов без x-user-id"
    );
    assert!(
        budget_of(&request).is_none(),
        "без срока нет и grpc-timeout"
    );
    let mut late = Stamp::new("r").until(tokio::time::Instant::now());
    let refused = late
        .call(Request::new(()))
        .expect_err("бюджет кончился до вызова");
    assert_eq!(
        refused.code(),
        Code::DeadlineExceeded,
        "вызов, которому уже некогда, не уходит в сеть"
    );
    let mut endless = Stamp::fresh().within(Duration::from_millis(5)).endless();
    let request = endless.call(Request::new(())).expect("поток без срока");
    assert!(
        budget_of(&request).is_none(),
        "endless снимает срок для потоков"
    );
    assert_eq!(
        endless.request_id().len(),
        16,
        "fresh берёт новый x-request-id"
    );
}

#[test]
fn stamp_follows_an_incoming_call() {
    let mut incoming = Request::new(());
    let md = incoming.metadata_mut();
    md.insert(REQUEST_ID, "trace-9".parse().expect("ascii"));
    md.insert(USER_ID, "42".parse().expect("ascii"));
    md.insert("grpc-timeout", "300m".parse().expect("ascii"));
    let stamp = Stamp::follow(&incoming);
    assert_eq!(stamp.request_id(), "trace-9");
    assert_eq!(stamp.user_id(), Some(42));
    let left = stamp.left().expect("остаток срока переносится дальше");
    assert!(
        left <= Duration::from_millis(300) && left > Duration::from_millis(200),
        "{left:?}"
    );
    let mut called = Request::new(());
    called.extensions_mut().insert(Caller(5));
    assert_eq!(
        Stamp::follow(&called).user_id(),
        Some(5),
        "пользователь из extensions, если перехватчик сервера уже его положил"
    );
    assert_eq!(Stamp::follow(&Request::new(())).request_id(), "-");
}

#[test]
fn caller_lands_in_extensions() {
    let mut request = Request::new(());
    request
        .metadata_mut()
        .insert(USER_ID, "5".parse().expect("ascii"));
    let request = caller(request).expect("x-user-id число");
    assert_eq!(request.extensions().get::<Caller>(), Some(&Caller(5)));
    assert_eq!(caller_of(&request).ok(), Some(5));
    let mut garbage = Request::new(());
    garbage
        .metadata_mut()
        .insert(USER_ID, "alice".parse().expect("ascii"));
    let refused = caller(garbage).expect_err("x-user-id не число");
    assert_eq!(refused.code(), Code::InvalidArgument);
    let anonymous = caller(Request::new(())).expect("без x-user-id вызов проходит дальше");
    let missing = caller_of(&anonymous).expect_err("но действовать некому");
    assert_eq!(
        (missing.code(), domain_code(&missing)),
        (Code::Unauthenticated, Some("unauthorized"))
    );
}

#[test]
fn grpc_timeout_is_parsed_by_the_spec() {
    let cases = [
        ("300m", Some(Duration::from_millis(300))),
        ("2S", Some(Duration::from_secs(2))),
        ("1M", Some(Duration::from_secs(60))),
        ("1H", Some(Duration::from_secs(3600))),
        ("500u", Some(Duration::from_micros(500))),
        ("7n", Some(Duration::from_nanos(7))),
        ("123456789m", None),
        ("m", None),
        ("10x", None),
        ("-5m", None),
    ];
    for (raw, expected) in cases {
        assert_eq!(parse_grpc_timeout(raw), expected, "grpc-timeout {raw:?}");
    }
}

async fn health_server(
    log: Option<LogSink>,
    stop: &CancellationToken,
    grace: Duration,
) -> (SocketAddr, HealthReporter) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let (reporter, health) = health_reporter();
    reporter
        .set_service_status(PROBE, ServingStatus::NotServing)
        .await;
    let stopping = drain::<Probe>(reporter.clone(), stop.clone(), grace);
    tokio::spawn(async move {
        Server::builder()
            .layer(AccessLog::new("probe", log))
            .add_service(health)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), stopping)
            .await
    });
    (addr, reporter)
}

fn channel(addr: SocketAddr) -> Channel {
    Channel::from_shared(format!("http://{addr}"))
        .expect("адрес")
        .connect_lazy()
}

#[tokio::test]
async fn access_log_writes_one_line_per_call() {
    let lines = Arc::new(Mutex::new(Vec::<Access>::new()));
    let sink = {
        let lines = Arc::clone(&lines);
        LogSink::new(move |access| {
            lines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(access.clone())
        })
    };
    let stop = CancellationToken::new();
    let (addr, _reporter) = health_server(Some(sink), &stop, Duration::ZERO).await;
    let stamp = Stamp::new("rid-1").user(3);
    let mut client = HealthClient::with_interceptor(channel(addr), stamp);
    let ask = |service: &str| HealthCheckRequest {
        service: service.to_string(),
    };
    client
        .check(ask(PROBE))
        .await
        .expect("проверка известного сервиса");
    let unknown = client
        .check(ask("nope.v1.Nope"))
        .await
        .expect_err("неизвестный");
    assert_eq!(unknown.code(), Code::NotFound);
    let seen = lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let short: Vec<(&str, &str, Option<&str>, &str, i32)> = seen
        .iter()
        .map(|a| {
            (
                a.service,
                a.request_id.as_str(),
                a.user.as_deref(),
                a.path.as_str(),
                a.code,
            )
        })
        .collect();
    assert_eq!(
        short,
        [
            (
                "probe",
                "rid-1",
                Some("3"),
                "/grpc.health.v1.Health/Check",
                0
            ),
            (
                "probe",
                "rid-1",
                Some("3"),
                "/grpc.health.v1.Health/Check",
                5
            ),
        ],
        "слой видит запрос и ответ целиком: путь, x-request-id, x-user-id и код"
    );
    let line = seen[1].to_string();
    assert!(
        line.starts_with("probe [rid-1] u=3 /grpc.health.v1.Health/Check 5 ")
            && line.ends_with("ms"),
        "строка журнала: {line}"
    );
    stop.cancel();
}

#[tokio::test]
async fn readiness_follows_health_and_drain_says_not_serving_first() {
    let stop = CancellationToken::new();
    let (addr, reporter) = health_server(None, &stop, Duration::from_millis(300)).await;
    let ready = Readiness::watch(
        vec![Upstream {
            label: "probe-service",
            service: PROBE,
            channel: channel(addr),
        }],
        &stop,
    );
    assert_eq!(
        ready.missing(),
        ["probe-service"],
        "до SERVING сервис не готов"
    );
    assert_eq!(
        ready.wait(Duration::from_millis(200)).await,
        Err(vec!["probe-service"]),
        "NOT_SERVING держит готовность"
    );
    reporter
        .set_service_status(PROBE, ServingStatus::Serving)
        .await;
    ready
        .wait(WAIT)
        .await
        .expect("SERVING приходит по потоку Watch, без опроса");
    assert!(ready.is_ready());
    let mut client = HealthClient::new(channel(addr));
    let ask = HealthCheckRequest {
        service: PROBE.to_string(),
    };
    stop.cancel();
    let during = client
        .check(ask)
        .await
        .expect("в паузе grace сервер ещё отвечает");
    assert_eq!(
        during.into_inner().status(),
        Seen::NotServing,
        "после сигнала остановки сразу NOT_SERVING, и только потом сервер закрывается"
    );
}
