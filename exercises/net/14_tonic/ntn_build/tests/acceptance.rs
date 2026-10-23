use ntn_build::users::v1::users_client::UsersClient;
use ntn_build::users::v1::users_server::{Users, UsersServer};
use ntn_build::users::v1::{GetUserRequest, User};
use prost::Message;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::server::NamedService;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

struct Fake;

#[tonic::async_trait]
impl Users for Fake {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        Ok(Response::new(User { id, handle: format!("user{id}"), display_name: String::new() }))
    }
}

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

#[test]
fn service_name_matches_the_package() {
    assert_eq!(
        <UsersServer<Fake> as NamedService>::NAME,
        "users.v1.Users",
        "имя сервиса складывается из пакета и сервиса в .proto: package users.v1, service Users"
    );
}

#[test]
fn user_message_has_the_agreed_field_numbers() {
    let user = User { id: 1, handle: "ann".to_string(), display_name: "A".to_string() };
    assert_eq!(
        user.encode_to_vec(),
        vec![0x08, 0x01, 0x12, 0x03, b'a', b'n', b'n', 0x1a, 0x01, b'A'],
        "номера полей User: id = 1, handle = 2, display_name = 3"
    );
}

#[tokio::test]
async fn trait_can_be_called_without_network() {
    let reply = Fake.get_user(Request::new(GetUserRequest { id: 7 })).await.unwrap().into_inner();
    assert_eq!(reply.handle, "user7");
}

#[tokio::test]
async fn real_client_talks_to_the_service_over_the_network() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().add_service(UsersServer::new(Fake)).serve_with_incoming(TcpListenerStream::new(listener)));
    let mut client = within("подключение", UsersClient::connect(format!("http://{addr}"))).await.unwrap();
    let user = within("вызов", client.get_user(GetUserRequest { id: 42 })).await.unwrap().into_inner();
    assert_eq!((user.id, user.handle.as_str()), (42, "user42"), "ответ сервера дошёл до клиента искажённым");
}

#[test]
fn build_script_reruns_when_the_schema_changes() {
    let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).expect("нет build.rs");
    assert!(script.contains("cargo:rerun-if-changed"), "без rerun-if-changed cargo не заметит правку .proto и будет жить со старым кодом");
}
