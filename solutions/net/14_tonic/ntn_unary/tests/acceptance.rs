use ntn_unary::pb::users_client::UsersClient;
use ntn_unary::pb::users_server::Users;
use ntn_unary::pb::{CreateUserRequest, GetUserRequest};
use ntn_unary::{start, UsersSvc};
use std::collections::HashSet;
use std::time::Duration;
use tonic::{Code, Request};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn create(handle: &str) -> CreateUserRequest {
    CreateUserRequest { handle: handle.to_string(), display_name: String::new() }
}

#[tokio::test]
async fn handlers_work_without_network() {
    let svc = UsersSvc::new();
    let ann = svc.create_user(Request::new(create("ann"))).await.unwrap().into_inner();
    let bob = svc.create_user(Request::new(create("bob"))).await.unwrap().into_inner();
    assert_eq!((ann.id, bob.id), (1, 2), "номера выдаются по порядку, начиная с 1");
    let got = svc.get_user(Request::new(GetUserRequest { id: 2 })).await.unwrap().into_inner();
    assert_eq!(got.handle, "bob", "get_user вернул не того пользователя: ищи по номеру из запроса");
    let missing = svc.get_user(Request::new(GetUserRequest { id: 99 })).await.unwrap_err();
    assert_eq!(missing.code(), Code::NotFound, "неизвестный номер это NotFound, а не пустой пользователь");
}

#[tokio::test]
async fn client_talks_to_the_server_over_the_network() {
    let (addr, _server) = start(UsersSvc::new()).await.unwrap();
    let mut client = within("подключение", UsersClient::connect(format!("http://{addr}"))).await.unwrap();
    let ann = within("create", client.create_user(create("ann"))).await.unwrap().into_inner();
    let again = within("get", client.get_user(GetUserRequest { id: ann.id })).await.unwrap().into_inner();
    assert_eq!(again.handle, "ann");
}

#[tokio::test]
async fn forty_parallel_creates_give_forty_distinct_ids() {
    let (addr, _server) = start(UsersSvc::new()).await.unwrap();
    let client = within("подключение", UsersClient::connect(format!("http://{addr}"))).await.unwrap();
    let mut tasks = Vec::new();
    for i in 0..40 {
        let mut client = client.clone();
        tasks.push(tokio::spawn(async move { client.create_user(create(&format!("user{i}"))).await.unwrap().into_inner().id }));
    }
    let mut ids = HashSet::new();
    for task in tasks {
        ids.insert(within("параллельный create", task).await.unwrap());
    }
    assert_eq!(ids.len(), 40, "два параллельных create получили один номер: чтение и запись номера должны идти под одним замком");
}

#[tokio::test]
async fn two_servers_do_not_share_ports_or_users() {
    let (a, _sa) = start(UsersSvc::new()).await.unwrap();
    let (b, _sb) = start(UsersSvc::new()).await.unwrap();
    assert_ne!(a, b, "оба сервера заняли один порт: нужен порт 0, чтобы выбрала система");
    let mut ca = UsersClient::connect(format!("http://{a}")).await.unwrap();
    let mut cb = UsersClient::connect(format!("http://{b}")).await.unwrap();
    within("create", ca.create_user(create("ann"))).await.unwrap();
    let err = within("get", cb.get_user(GetUserRequest { id: 1 })).await.unwrap_err();
    assert_eq!(err.code(), Code::NotFound, "у каждого сервера своё хранилище");
}

#[tokio::test]
async fn port_is_open_as_soon_as_start_returns() {
    let (addr, _server) = start(UsersSvc::new()).await.unwrap();
    let connect = tokio::net::TcpStream::connect(addr);
    within("TCP-подключение сразу после start", connect).await.expect("слушатель должен быть открыт до возврата из start");
}

#[tokio::test]
async fn abort_closes_the_port() {
    let (addr, server) = start(UsersSvc::new()).await.unwrap();
    server.abort();
    let mut closed = false;
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_err() {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(closed, "после abort порт всё ещё принимает соединения: сервер должен работать в задаче, которую возвращает start");
}
