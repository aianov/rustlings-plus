use ntn_status::pb::users_client::UsersClient;
use ntn_status::pb::users_server::{Users, UsersServer};
use ntn_status::pb::{CreateUserRequest, GetUserRequest, User};
use ntn_status::{find_user, register, start, Registration, UsersSvc};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn real_client() -> UsersClient<Channel> {
    let (addr, _server) = start(UsersSvc::new()).await.unwrap();
    UsersClient::connect(format!("http://{addr}")).await.unwrap()
}

fn create(handle: &str) -> CreateUserRequest {
    CreateUserRequest { handle: handle.to_string(), display_name: String::new() }
}

struct Scripted(Code);

#[tonic::async_trait]
impl Users for Scripted {
    async fn get_user(&self, _: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::new(self.0, "заготовленная ошибка"))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::new(self.0, "заготовленная ошибка"))
    }
}

async fn scripted_client(code: Code) -> UsersClient<Channel> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().add_service(UsersServer::new(Scripted(code))).serve_with_incoming(TcpListenerStream::new(listener)));
    UsersClient::connect(format!("http://{addr}")).await.unwrap()
}

#[tokio::test]
async fn unknown_user_is_not_found_with_the_id_in_the_message() {
    let mut client = real_client().await;
    let err = within("get", client.get_user(GetUserRequest { id: 777 })).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound, "неизвестный номер это NotFound");
    assert!(err.message().contains("777"), "в сообщении должен быть номер, чтобы по логу было понятно, кого искали: {}", err.message());
}

#[tokio::test]
async fn bad_handles_are_invalid_argument_and_name_the_field() {
    let mut client = real_client().await;
    for bad in ["", "ab", "a_very_long_handle_over_twenty", "ann smith", "анна_12", "ann!"] {
        let err = within("create", client.create_user(create(bad))).await.unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument, "ник {bad:?} недопустим, ждали InvalidArgument, получили {err}");
        assert!(err.message().contains("handle"), "в сообщении должно быть имя поля handle: {}", err.message());
    }
    for good in ["abc", "ann_2", "x".repeat(20).as_str()] {
        within("create", client.create_user(create(good))).await.unwrap_or_else(|e| panic!("ник {good:?} допустим, но вернулась ошибка {e}"));
    }
}

#[tokio::test]
async fn handles_are_unique_ignoring_case() {
    let mut client = real_client().await;
    let first = within("create", client.create_user(create("Ann_1"))).await.unwrap().into_inner();
    assert_eq!(first.handle, "Ann_1", "ник сохраняется как написан");
    for same in ["Ann_1", "ann_1", "ANN_1"] {
        let err = within("create", client.create_user(create(same))).await.unwrap_err();
        assert_eq!(err.code(), Code::AlreadyExists, "ник {same:?} совпадает с Ann_1 без учёта регистра");
    }
}

#[tokio::test]
async fn find_user_turns_not_found_into_none() {
    let mut client = real_client().await;
    assert_eq!(within("find", find_user(&mut client, 5)).await.unwrap(), None, "отсутствие пользователя это не ошибка");
    let ann = client.create_user(create("ann")).await.unwrap().into_inner();
    assert_eq!(within("find", find_user(&mut client, ann.id)).await.unwrap(), Some(ann));
}

#[tokio::test]
async fn find_user_does_not_hide_real_failures() {
    for code in [Code::Unavailable, Code::Internal, Code::DeadlineExceeded, Code::PermissionDenied] {
        let mut client = scripted_client(code).await;
        let err = within("find", find_user(&mut client, 1)).await.expect_err("поломка сервиса не должна превращаться в «пользователя нет»");
        assert_eq!(err.code(), code, "код ошибки потерялся по дороге");
    }
}

#[tokio::test]
async fn register_reports_taken_handle_as_a_value() {
    let mut client = real_client().await;
    let first = within("register", register(&mut client, "ann")).await.unwrap();
    assert!(matches!(first, Registration::Created(ref u) if u.handle == "ann" && u.id == 1), "первая регистрация должна пройти: {first:?}");
    assert_eq!(within("register", register(&mut client, "ANN")).await.unwrap(), Registration::HandleTaken);
}

#[tokio::test]
async fn register_passes_every_other_error_through() {
    for code in [Code::InvalidArgument, Code::Unavailable, Code::Internal] {
        let mut client = scripted_client(code).await;
        let err = within("register", register(&mut client, "ann")).await.expect_err("только AlreadyExists становится значением");
        assert_eq!(err.code(), code, "код ошибки потерялся по дороге");
    }
}
