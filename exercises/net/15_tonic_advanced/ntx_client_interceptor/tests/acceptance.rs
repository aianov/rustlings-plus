use ntx_client_interceptor::pb::users_server::{Users, UsersServer};
use ntx_client_interceptor::pb::{CreateUserRequest, GetUserRequest, User};
use ntx_client_interceptor::{authed_client, SharedToken};
use std::sync::{Arc, Mutex, RwLock};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<Option<String>>>>);

#[tonic::async_trait]
impl Users for Recorder {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let seen = request.metadata().get("authorization").and_then(|v| v.to_str().ok()).map(str::to_string);
        self.0.lock().unwrap().push(seen);
        Ok(Response::new(User { id: request.into_inner().id, handle: "x".to_string(), display_name: String::new() }))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

async fn setup() -> (Channel, Recorder) {
    let rec = Recorder::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().add_service(UsersServer::new(rec.clone())).serve_with_incoming(TcpListenerStream::new(listener)));
    (Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy(), rec)
}

fn token(s: &str) -> SharedToken {
    Arc::new(RwLock::new(s.to_string()))
}

#[tokio::test]
async fn every_call_carries_the_bearer_token() {
    let (channel, rec) = setup().await;
    let mut client = authed_client(channel, token("abc.def"));
    for id in 1..=3 {
        within("вызов", client.get_user(GetUserRequest { id })).await.unwrap();
    }
    let seen = rec.0.lock().unwrap().clone();
    assert_eq!(seen, vec![Some("Bearer abc.def".to_string()); 3], "каждый вызов должен нести заголовок authorization: Bearer <токен>");
}

#[tokio::test]
async fn refreshed_token_is_picked_up_without_a_new_client() {
    let (channel, rec) = setup().await;
    let shared = token("old");
    let mut client = authed_client(channel, shared.clone());
    within("вызов", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    *shared.write().unwrap() = "new".to_string();
    within("вызов", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    let seen = rec.0.lock().unwrap().clone();
    assert_eq!(seen, vec![Some("Bearer old".to_string()), Some("Bearer new".to_string())], "токен надо читать в момент вызова, а не копировать при создании клиента");
}

#[tokio::test]
async fn explicit_header_wins_over_the_shared_token() {
    let (channel, rec) = setup().await;
    let mut client = authed_client(channel, token("shared"));
    let mut request = Request::new(GetUserRequest { id: 1 });
    request.metadata_mut().insert("authorization", MetadataValue::from_static("Bearer special"));
    within("вызов", client.get_user(request)).await.unwrap();
    assert_eq!(rec.0.lock().unwrap().clone(), vec![Some("Bearer special".to_string())], "явный заголовок вызывающего нельзя перетирать");
}

#[tokio::test]
async fn empty_token_means_anonymous() {
    let (channel, rec) = setup().await;
    let mut client = authed_client(channel, token(""));
    within("вызов", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    assert_eq!(rec.0.lock().unwrap().clone(), vec![None], "без токена заголовка быть не должно (а не «Bearer » с пустотой)");
}

#[tokio::test]
async fn bad_token_fails_before_sending() {
    let (channel, rec) = setup().await;
    for bad in ["line\nbreak", "tab\there", "пароль"] {
        let mut client = authed_client(channel.clone(), token(bad));
        let err = within("вызов", client.get_user(GetUserRequest { id: 1 })).await.expect_err("токен с недопустимыми символами не должен уезжать");
        assert_eq!(err.code(), Code::Unauthenticated, "для токена {bad:?} ждали Unauthenticated, получили {err}");
    }
    assert!(rec.0.lock().unwrap().is_empty(), "до сервера плохой токен дойти не должен");
}
