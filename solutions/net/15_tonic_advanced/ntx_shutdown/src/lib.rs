// Остановка сервиса по-человечески: сначала NOT_SERVING, потом пауза, пока балансировщик заметит,
// потом остановка, но вызовы в полёте доживают. Допиши run.
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tonic_health::server::health_reporter;
use tonic_health::ServingStatus;

pub mod pb {
    tonic::include_proto!("work.v1");
}
use pb::work_server::{Work, WorkServer};
use pb::{RunReply, RunRequest};

pub const SERVICE: &str = "work.v1.Work";

struct WorkSvc;

#[tonic::async_trait]
impl Work for WorkSvc {
    async fn run(&self, request: Request<RunRequest>) -> Result<Response<RunReply>, Status> {
        tokio::time::sleep(Duration::from_millis(request.into_inner().ms)).await;
        Ok(Response::new(RunReply {}))
    }
}

pub async fn run(listener: TcpListener, shutdown: oneshot::Receiver<()>, grace: Duration) -> Result<(), tonic::transport::Error> {
    let (reporter, health) = health_reporter();
    reporter.set_service_status(SERVICE, ServingStatus::Serving).await;
    let signal = async move {
        let _ = shutdown.await;
        reporter.set_service_status(SERVICE, ServingStatus::NotServing).await;
        tokio::time::sleep(grace).await;
    };
    Server::builder().add_service(health).add_service(WorkServer::new(WorkSvc)).serve_with_incoming_shutdown(TcpListenerStream::new(listener), signal).await
}
