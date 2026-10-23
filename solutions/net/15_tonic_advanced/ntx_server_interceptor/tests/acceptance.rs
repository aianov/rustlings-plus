use ntx_server_interceptor::pb::me_client::MeClient;
use ntx_server_interceptor::pb::public_client::PublicClient;
use ntx_server_interceptor::pb::{PingRequest, WhoAmIRequest};
use ntx_server_interceptor::{start, Auth};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::{Code, Request};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn setup() -> (Channel, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let addr = start(Auth::new(&[("t-ann", 1), ("t-bob", 2)]), calls.clone()).await.unwrap();
    let channel = tonic::transport::Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
    (channel, calls)
}

fn with_header(value: &str) -> Request<WhoAmIRequest> {
    let mut request = Request::new(WhoAmIRequest {});
    request.metadata_mut().insert("authorization", MetadataValue::try_from(value).unwrap());
    request
}

#[tokio::test]
async fn valid_token_reaches_the_handler_with_the_user() {
    let (channel, calls) = setup().await;
    let mut me = MeClient::new(channel);
    let a = within("вызов", me.who_am_i(with_header("Bearer t-ann"))).await.unwrap().into_inner();
    let b = within("вызов", me.who_am_i(with_header("Bearer t-bob"))).await.unwrap().into_inner();
    assert_eq!((a.user_id, b.user_id), (1, 2), "обработчик должен получать номер владельца токена из расширений запроса");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn everything_else_is_unauthenticated_and_never_reaches_the_handler() {
    let (channel, calls) = setup().await;
    let mut me = MeClient::new(channel);
    let cases: Vec<(&str, Request<WhoAmIRequest>)> = vec![
        ("нет заголовка", Request::new(WhoAmIRequest {})),
        ("чужая схема", with_header("Basic dDphbm4=")),
        ("схема без токена", with_header("Bearer")),
        ("неизвестный токен", with_header("Bearer t-mallory")),
        ("не ASCII", with_header("Bearer пароль")),
        ("токен без схемы", with_header("t-ann")),
    ];
    for (what, request) in cases {
        let err = within("вызов", me.who_am_i(request)).await.expect_err(what);
        assert_eq!(err.code(), Code::Unauthenticated, "{what}: ждали Unauthenticated, получили {err}");
        assert!(!err.message().contains("mallory") && !err.message().contains("t-ann") && !err.message().contains("пароль"), "{what}: токен просочился в сообщение об ошибке: {}", err.message());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "обработчик не должен вызываться, если перехватчик отказал");
}

#[tokio::test]
async fn public_service_needs_no_token() {
    let (channel, _calls) = setup().await;
    let mut public = PublicClient::new(channel);
    let reply = within("ping", public.ping(PingRequest {})).await.unwrap().into_inner();
    assert_eq!(reply.text, "pong", "открытый сервис должен отвечать без токена: перехватчик навешивается только на закрытый");
}
