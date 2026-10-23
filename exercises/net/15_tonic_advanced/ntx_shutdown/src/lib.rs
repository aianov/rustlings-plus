// Остановка сервиса по-человечески: сначала NOT_SERVING, потом пауза, пока балансировщик заметит,
// потом остановка, но вызовы в полёте доживают. Допиши run.
#![allow(unused_variables)]

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
    todo!()
}
