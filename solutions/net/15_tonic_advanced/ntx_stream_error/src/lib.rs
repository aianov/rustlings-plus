// Экспорт потоком. Когда источник падает посреди выгрузки, клиент должен узнать об этом, а не получить
// «успешный» неполный результат. Дефектов два, по одному в сервере и клиенте.
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("export.v1");
}
use pb::export_client::ExportClient;
use pb::export_server::{Export, ExportServer};
use pb::{Row, RowsRequest};

pub struct ExportSvc {
    pub fail_at: Option<u64>,
}

#[tonic::async_trait]
impl Export for ExportSvc {
    type RowsStream = ReceiverStream<Result<Row, Status>>;

    async fn rows(&self, request: Request<RowsRequest>) -> Result<Response<Self::RowsStream>, Status> {
        let count = request.into_inner().count;
        let fail_at = self.fail_at;
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            for id in 1..=count {
                if Some(id) == fail_at {
                    eprintln!("экспорт: источник упал на строке {id}");
                    let _ = tx.send(Err(Status::unavailable(format!("источник данных упал на строке {id}")))).await;
                    return;
                }
                if tx.send(Ok(Row { id, text: format!("строка {id}") })).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub async fn start(fail_at: Option<u64>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(ExportServer::new(ExportSvc { fail_at })).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}

#[derive(Debug)]
pub enum ExportError {
    Truncated { got: usize, cause: Status },
    Failed(Status),
}

pub async fn collect_rows(client: &mut ExportClient<Channel>, count: u64) -> Result<Vec<Row>, ExportError> {
    let mut stream = client.rows(RowsRequest { count }).await.map_err(ExportError::Failed)?.into_inner();
    let mut rows = Vec::new();
    loop {
        match stream.message().await {
            Ok(Some(row)) => rows.push(row),
            Ok(None) => return Ok(rows),
            Err(cause) => return Err(ExportError::Truncated { got: rows.len(), cause }),
        }
    }
}
