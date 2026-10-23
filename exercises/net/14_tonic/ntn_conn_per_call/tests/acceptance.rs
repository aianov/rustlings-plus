use ntn_conn_per_call::pb::users_server::{Users, UsersServer};
use ntn_conn_per_call::pb::{CreateUserRequest, GetUserRequest, User};
use ntn_conn_per_call::Gateway;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

struct Slow;

#[tonic::async_trait]
impl Users for Slow {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let id = request.into_inner().id;
        Ok(Response::new(User { id, handle: format!("user{id}"), display_name: String::new() }))
    }
    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

async fn counting_server() -> (std::net::SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    let incoming = TcpListenerStream::new(listener).map(move |conn| {
        if conn.is_ok() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        conn
    });
    tokio::spawn(Server::builder().add_service(UsersServer::new(Slow)).serve_with_incoming(incoming));
    (addr, accepted)
}

async fn wave(gateway: &Arc<Gateway>, n: u64) -> Duration {
    let started = Instant::now();
    let mut tasks = Vec::new();
    for id in 1..=n {
        let gateway = gateway.clone();
        tasks.push(tokio::spawn(async move { (id, gateway.handle_of(id).await) }));
    }
    for task in tasks {
        let (id, handle) = tokio::time::timeout(Duration::from_secs(10), task).await.expect("вызов завис").unwrap();
        assert_eq!(handle.unwrap(), format!("user{id}"), "ответ пришёл не на тот вызов");
    }
    started.elapsed()
}

#[tokio::test]
async fn twenty_parallel_calls_use_one_connection() {
    let (addr, accepted) = counting_server().await;
    let gateway = Arc::new(Gateway::new(addr));
    wave(&gateway, 20).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "на двадцать вызовов открыто больше одного соединения: клиент должен создаваться один раз");
}

#[tokio::test]
async fn parallel_calls_stay_parallel() {
    let (addr, _accepted) = counting_server().await;
    let gateway = Arc::new(Gateway::new(addr));
    wave(&gateway, 1).await;
    let took = wave(&gateway, 20).await;
    assert!(took < Duration::from_millis(1500), "двадцать вызовов по 200 мс заняли {took:?}: они выстроились в очередь, общий замок на клиенте ни к чему");
}

#[tokio::test]
async fn second_wave_reuses_the_connection() {
    let (addr, accepted) = counting_server().await;
    let gateway = Arc::new(Gateway::new(addr));
    wave(&gateway, 20).await;
    wave(&gateway, 20).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "вторая волна открыла новые соединения");
}

#[tokio::test]
async fn gateway_can_be_created_before_the_service_exists() {
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap()
    };
    let gateway = Gateway::new(dead);
    let err = tokio::time::timeout(Duration::from_secs(3), gateway.handle_of(1)).await.expect("вызов завис").unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unavailable);
}
