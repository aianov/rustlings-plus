use ntx_tower_layer::pb::users_client::UsersClient;
use ntx_tower_layer::pb::users_server::{Users, UsersServer};
use ntx_tower_layer::pb::{CreateUserRequest, GetUserRequest, User};
use ntx_tower_layer::{LogLayer, LogLine, Sink};
use std::sync::Arc;
use std::time::Duration;
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

struct Svc;

#[tonic::async_trait]
impl Users for Svc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        tokio::time::sleep(Duration::from_millis(100)).await;
        if id == 404 {
            return Err(Status::not_found("нет такого"));
        }
        Ok(Response::new(User { id, handle: format!("u{id}"), display_name: String::new() }))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

async fn setup() -> (UsersClient<Channel>, Sink) {
    let sink: Sink = Arc::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().layer(LogLayer::new(sink.clone())).add_service(UsersServer::new(Svc)).serve_with_incoming(TcpListenerStream::new(listener)));
    (UsersClient::new(Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()), sink)
}

fn with_id(id: u64, rid: &str) -> Request<GetUserRequest> {
    let mut request = Request::new(GetUserRequest { id });
    request.metadata_mut().insert("x-request-id", MetadataValue::try_from(rid).unwrap());
    request
}

fn lines(sink: &Sink) -> Vec<LogLine> {
    sink.lock().unwrap().clone()
}

#[tokio::test]
async fn successful_call_is_logged_with_id_path_and_time() {
    let (mut client, sink) = setup().await;
    let reply = within("вызов", client.get_user(with_id(5, "req-1"))).await.unwrap().into_inner();
    assert_eq!(reply.handle, "u5", "слой не должен менять ответ");
    let l = lines(&sink);
    assert_eq!(l.len(), 1, "одна строка на вызов: {l:?}");
    assert_eq!((l[0].request_id.as_str(), l[0].path.as_str(), l[0].status), ("req-1", "/users.v1.Users/GetUser", 0));
    assert!(l[0].elapsed >= Duration::from_millis(90), "обработчик работает 100 мс, а в журнале {:?}: время надо мерить вокруг вызова внутреннего сервиса", l[0].elapsed);
}

#[tokio::test]
async fn missing_request_id_is_a_dash() {
    let (mut client, sink) = setup().await;
    within("вызов", client.get_user(GetUserRequest { id: 1 })).await.unwrap();
    assert_eq!(lines(&sink)[0].request_id, "-", "без заголовка в журнале ставится «-»");
}

#[tokio::test]
async fn error_status_is_visible_in_the_log() {
    let (mut client, sink) = setup().await;
    let err = within("вызов", client.get_user(with_id(404, "req-2"))).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound, "слой не должен менять и ошибки");
    let l = lines(&sink);
    assert_eq!(l[0].status, Code::NotFound as i32, "код ошибки (grpc-status = 5) должен попасть в журнал: {l:?}");
}

#[tokio::test]
async fn parallel_calls_each_get_their_own_line() {
    let (client, sink) = setup().await;
    let mut tasks = Vec::new();
    for i in 0..20u64 {
        let mut client = client.clone();
        tasks.push(tokio::spawn(async move { client.get_user(with_id(i, &format!("par-{i}"))).await.map(|_| ()) }));
    }
    for task in tasks {
        within("вызов", task).await.unwrap().unwrap();
    }
    let mut ids: Vec<String> = lines(&sink).into_iter().map(|l| l.request_id).collect();
    ids.sort();
    let mut expected: Vec<String> = (0..20).map(|i| format!("par-{i}")).collect();
    expected.sort();
    assert_eq!(ids, expected, "на каждый из двадцати вызовов должна быть своя строка со своим идентификатором");
}
