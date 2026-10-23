use ntx_stream_error::pb::export_client::ExportClient;
use ntx_stream_error::pb::export_server::{Export, ExportServer};
use ntx_stream_error::pb::{Row, RowsRequest};
use ntx_stream_error::{collect_rows, start, ExportError};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn channel(addr: std::net::SocketAddr) -> Channel {
    Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_lazy()
}

struct Scripted;

#[tonic::async_trait]
impl Export for Scripted {
    type RowsStream = tokio_stream::Iter<std::vec::IntoIter<Result<Row, Status>>>;

    async fn rows(&self, _: Request<RowsRequest>) -> Result<Response<Self::RowsStream>, Status> {
        let items: Vec<Result<Row, Status>> = vec![
            Ok(Row { id: 1, text: "a".into() }),
            Ok(Row { id: 2, text: "b".into() }),
            Ok(Row { id: 3, text: "c".into() }),
            Err(Status::data_loss("заготовленный сбой")),
        ];
        Ok(Response::new(tokio_stream::iter(items)))
    }
}

#[tokio::test]
async fn server_reports_a_failed_source_as_an_error_item() {
    let addr = start(Some(4)).await.unwrap();
    let mut client = ExportClient::new(channel(addr));
    let mut stream = within("вызов", client.rows(RowsRequest { count: 10 })).await.unwrap().into_inner();
    let mut ok_rows = 0;
    let last = loop {
        match within("чтение", stream.message()).await {
            Ok(Some(_)) => ok_rows += 1,
            Ok(None) => break None,
            Err(status) => break Some(status),
        }
    };
    assert_eq!(ok_rows, 3, "до сбоя должны дойти три строки");
    let status = last.expect("поток закончился как нормальный, хотя источник упал: клиент не узнает о неполноте. В поток надо положить Err(Status)");
    assert_eq!(status.code(), Code::Unavailable);
}

#[tokio::test]
async fn client_does_not_pass_a_truncated_stream_off_as_complete() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(Server::builder().add_service(ExportServer::new(Scripted)).serve_with_incoming(TcpListenerStream::new(listener)));
    let mut client = ExportClient::new(channel(addr));
    match within("сбор", collect_rows(&mut client, 10)).await {
        Err(ExportError::Truncated { got, cause }) => {
            assert_eq!(got, 3, "до ошибки пришло три строки");
            assert_eq!(cause.code(), Code::DataLoss, "причина должна доехать без искажений");
        }
        other => panic!("ждали Truncated, получили {other:?}: ошибка посреди потока не должна превращаться в «успешный» неполный список"),
    }
}

#[tokio::test]
async fn complete_export_returns_every_row() {
    let addr = start(None).await.unwrap();
    let mut client = ExportClient::new(channel(addr));
    let rows = within("сбор", collect_rows(&mut client, 10)).await.expect("без сбоев экспорт должен удаться");
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), (1..=10).collect::<Vec<_>>());
}

#[tokio::test]
async fn both_halves_together() {
    let addr = start(Some(4)).await.unwrap();
    let mut client = ExportClient::new(channel(addr));
    match within("сбор", collect_rows(&mut client, 10)).await {
        Err(ExportError::Truncated { got: 3, cause }) => assert_eq!(cause.code(), Code::Unavailable),
        other => panic!("ждали Truncated {{ got: 3, Unavailable }}, получили {other:?}"),
    }
}

#[tokio::test]
async fn failure_before_the_first_row_is_a_plain_failure() {
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap()
    };
    let mut client = ExportClient::new(Endpoint::from_shared(format!("http://{dead}")).unwrap().connect_timeout(Duration::from_millis(300)).connect_lazy());
    match within("сбор", collect_rows(&mut client, 10)).await {
        Err(ExportError::Failed(status)) => assert_eq!(status.code(), Code::Unavailable),
        other => panic!("ждали Failed(Unavailable), получили {other:?}"),
    }
}
