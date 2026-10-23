use ntx_deadline::pb::search_client::SearchClient;
use ntx_deadline::pb::search_server::{Search, SearchServer};
use ntx_deadline::pb::{FindReply, FindRequest};
use ntx_deadline::{find_within, parse_grpc_timeout, start, Outcome, SearchSvc};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn header_values_are_parsed_by_unit() {
    let ok = [
        ("1H", Duration::from_secs(3600)),
        ("2M", Duration::from_secs(120)),
        ("5S", Duration::from_secs(5)),
        ("100m", ms(100)),
        ("250u", Duration::from_micros(250)),
        ("1500n", Duration::from_nanos(1500)),
        ("0S", Duration::ZERO),
        ("99999999S", Duration::from_secs(99_999_999)),
    ];
    for (text, want) in ok {
        assert_eq!(parse_grpc_timeout(text), Some(want), "значение {text:?} разобрано неверно");
    }
    for bad in ["", "S", "5", "5s", "5X", "-5S", "1.5S", "123456789S", " 5S", "5S ", "ёS", "5 S", "١S"] {
        assert_eq!(parse_grpc_timeout(bad), None, "значение {bad:?} недопустимо");
    }
}

async fn client_for(svc: SearchSvc) -> (SearchClient<Channel>, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let calls = svc.backend_calls.clone();
    let addr = start(svc).await.unwrap();
    (SearchClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()), calls)
}

#[tokio::test]
async fn too_little_time_is_refused_without_touching_the_backend() {
    let (mut client, calls) = client_for(SearchSvc::new(ms(100))).await;
    let started = Instant::now();
    let outcome = within("find", find_within(&mut client, "rust", ms(50))).await;
    assert!(matches!(outcome, Outcome::TimedOut), "при остатке 50 мс и цене 100 мс ждали TimedOut, получили {outcome:?}");
    assert!(started.elapsed() < ms(40) + ms(50), "отказ должен прийти сразу, а не по истечении срока");
    tokio::time::sleep(ms(200)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0, "бэкенд вызван, хотя заведомо не успеваем: остаток надо проверять до работы");
}

#[tokio::test]
async fn enough_time_goes_to_the_backend_once() {
    let (mut client, calls) = client_for(SearchSvc::new(ms(100))).await;
    match within("find", find_within(&mut client, "rust", Duration::from_secs(1))).await {
        Outcome::Hits(hits) => assert_eq!(hits, vec!["результат для rust".to_string()]),
        other => panic!("ждали результат, получили {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn call_without_deadline_just_works() {
    let (mut client, calls) = client_for(SearchSvc::new(ms(20))).await;
    let reply = within("find", client.find(FindRequest { query: "x".into() })).await.unwrap().into_inner();
    assert_eq!(reply.hits.len(), 1, "без срока сервис отвечает как обычно");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

struct Scripted(Code);

#[tonic::async_trait]
impl Search for Scripted {
    async fn find(&self, _: Request<FindRequest>) -> Result<Response<FindReply>, Status> {
        Err(Status::new(self.0, "заготовка"))
    }
}

#[tokio::test]
async fn client_helper_tells_timeouts_from_failures() {
    for (code, timed_out) in [(Code::DeadlineExceeded, true), (Code::Cancelled, true), (Code::Unavailable, false), (Code::Internal, false)] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(Server::builder().add_service(SearchServer::new(Scripted(code))).serve_with_incoming(TcpListenerStream::new(listener)));
        let mut client = SearchClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy());
        let outcome = within("find", find_within(&mut client, "q", Duration::from_secs(1))).await;
        match (&outcome, timed_out) {
            (Outcome::TimedOut, true) => {}
            (Outcome::Failed(s), false) if s.code() == code => {}
            _ => panic!("код {code:?}: получили {outcome:?}, ждали {}", if timed_out { "TimedOut" } else { "Failed с тем же кодом" }),
        }
    }
}

#[tokio::test]
async fn the_deadline_really_travels_in_the_header() {
    struct Spy;
    #[tonic::async_trait]
    impl Search for Spy {
        async fn find(&self, request: Request<FindRequest>) -> Result<Response<FindReply>, Status> {
            let value = request.metadata().get("grpc-timeout").and_then(|v| v.to_str().ok()).unwrap_or("нет").to_string();
            Ok(Response::new(FindReply { hits: vec![value] }))
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().add_service(SearchServer::new(Spy)).serve_with_incoming(TcpListenerStream::new(listener)));
    let mut client = SearchClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy());
    let Outcome::Hits(hits) = within("find", find_within(&mut client, "q", Duration::from_secs(2))).await else { panic!("ждали ответ") };
    assert_ne!(hits[0], "нет", "срок вызова должен ехать в заголовке grpc-timeout, а не только жить в таймере клиента");
    assert_eq!(parse_grpc_timeout(&hits[0]).map(|d| d.as_secs()), Some(2));
}
