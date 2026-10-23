use ntn_client::pb::users_server::{Users, UsersServer};
use ntn_client::pb::{CreateUserRequest, GetUserRequest, User};
use ntn_client::{handle_of, make_channel};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Code, Request, Response, Status};

struct Svc;

#[tonic::async_trait]
impl Users for Svc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        if id == 2 {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        Ok(Response::new(User { id, handle: format!("user{id}"), display_name: String::new() }))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

async fn serve_on(listener: TcpListener) {
    let _ = Server::builder().add_service(UsersServer::new(Svc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
}

async fn dead_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

const CONNECT: Duration = Duration::from_millis(500);
const REQUEST: Duration = Duration::from_millis(300);

#[tokio::test]
async fn channel_for_a_dead_address_is_created_instantly_and_fails_fast_on_use() {
    let addr = dead_addr().await;
    let started = Instant::now();
    let channel = make_channel(addr, CONNECT, REQUEST);
    assert!(started.elapsed() < Duration::from_millis(200), "канал строился слишком долго: он не должен ждать сервис");
    let err = tokio::time::timeout(Duration::from_secs(3), handle_of(channel, 1)).await.expect("вызов завис").unwrap_err();
    assert_eq!(err.code(), Code::Unavailable, "нет сервиса, значит Unavailable: {err}");
}

#[tokio::test]
async fn fast_call_succeeds() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve_on(listener));
    let channel = make_channel(addr, CONNECT, REQUEST);
    let handle = tokio::time::timeout(Duration::from_secs(3), handle_of(channel, 1)).await.expect("вызов завис").unwrap();
    assert_eq!(handle, "user1");
}

#[tokio::test]
async fn slow_handler_is_cut_by_the_request_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve_on(listener));
    let channel = make_channel(addr, CONNECT, REQUEST);
    let started = Instant::now();
    let err = tokio::time::timeout(Duration::from_secs(3), handle_of(channel, 2))
        .await
        .expect("обработчик спит 3 секунды, а срок вызова 300 мс: клиент обязан сдаться сам")
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_millis(1500), "срок вызова не сработал");
    assert!(matches!(err.code(), Code::Cancelled | Code::DeadlineExceeded), "ждали обрыв по сроку, получили {err}");
}

#[tokio::test]
async fn channel_made_before_the_server_connects_by_itself_later() {
    let addr = dead_addr().await;
    let channel = make_channel(addr, CONNECT, REQUEST);
    assert!(handle_of(channel.clone(), 1).await.is_err(), "сервера ещё нет");
    let listener = TcpListener::bind(addr).await.unwrap();
    tokio::spawn(serve_on(listener));
    let mut handle = Err(Status::unknown("не пробовали"));
    for _ in 0..20 {
        handle = tokio::time::timeout(Duration::from_secs(2), handle_of(channel.clone(), 1)).await.expect("вызов завис");
        if handle.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(handle.unwrap(), "user1", "канал должен сам подключиться, когда сервис появился");
}
