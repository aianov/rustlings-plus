// Шлюз при старте должен дождаться, пока соседний сервис скажет «готов». Для этого у сервиса есть
// стандартный сервис здоровья. Допиши запуск, проверку и ожидание.
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::server::{health_reporter, HealthReporter};
pub use tonic_health::pb::health_check_response::ServingStatus as Observed;
pub use tonic_health::ServingStatus;

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_server::{Users, UsersServer};
use pb::{CreateUserRequest, GetUserRequest, User};

pub const SERVICE: &str = "users.v1.Users";

struct UsersSvc;

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        Ok(Response::new(User { id: request.into_inner().id, handle: "ann".to_string(), display_name: String::new() }))
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

pub async fn serve(listener: TcpListener, initial: ServingStatus) -> (HealthReporter, JoinHandle<()>) {
    let (reporter, health) = health_reporter();
    reporter.set_service_status(SERVICE, initial).await;
    let handle = tokio::spawn(async move {
        let _ = Server::builder().add_service(health).add_service(UsersServer::new(UsersSvc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    (reporter, handle)
}

pub async fn status_of(channel: &Channel, service: &str) -> Result<Observed, Status> {
    let mut client = HealthClient::new(channel.clone());
    let reply = client.check(HealthCheckRequest { service: service.to_string() }).await?.into_inner();
    Ok(reply.status())
}

pub async fn wait_serving(channel: &Channel, service: &str, timeout: Duration) -> Result<(), Status> {
    let wait = async {
        loop {
            if let Ok(Observed::Serving) = status_of(channel, service).await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(timeout, wait).await.map_err(|_| Status::deadline_exceeded(format!("{service} не стал SERVING за {timeout:?}")))
}
