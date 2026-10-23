// Клиент ставит вызову срок, и tonic передаёт его в заголовке grpc-timeout. Сервис может узнать остаток
// заранее и не начинать работу, которую всё равно бросят. Допиши разбор заголовка, обработчик и клиентский помощник.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

pub mod pb {
    tonic::include_proto!("search.v1");
}
use pb::search_client::SearchClient;
use pb::search_server::{Search, SearchServer};
use pb::{FindReply, FindRequest};

pub fn parse_grpc_timeout(value: &str) -> Option<Duration> {
    let unit = value.chars().last()?;
    let digits = &value[..value.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        'H' => Duration::from_secs(n * 3600),
        'M' => Duration::from_secs(n * 60),
        'S' => Duration::from_secs(n),
        'm' => Duration::from_millis(n),
        'u' => Duration::from_micros(n),
        'n' => Duration::from_nanos(n),
        _ => return None,
    })
}

pub fn budget_of<T>(request: &Request<T>) -> Option<Duration> {
    let value = request.metadata().get("grpc-timeout")?.to_str().ok()?;
    parse_grpc_timeout(value)
}

pub struct SearchSvc {
    cost: Duration,
    pub backend_calls: Arc<AtomicUsize>,
}

impl SearchSvc {
    pub fn new(cost: Duration) -> Self {
        SearchSvc { cost, backend_calls: Arc::default() }
    }
}

#[tonic::async_trait]
impl Search for SearchSvc {
    async fn find(&self, request: Request<FindRequest>) -> Result<Response<FindReply>, Status> {
        if let Some(budget) = budget_of(&request) {
            if budget < self.cost {
                return Err(Status::deadline_exceeded("времени осталось меньше, чем нужно бэкенду"));
            }
        }
        self.backend_calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.cost).await;
        Ok(Response::new(FindReply { hits: vec![format!("результат для {}", request.into_inner().query)] }))
    }
}

pub async fn start(svc: SearchSvc) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(SearchServer::new(svc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}

#[derive(Debug)]
pub enum Outcome {
    Hits(Vec<String>),
    TimedOut,
    Failed(Status),
}

pub async fn find_within(client: &mut SearchClient<Channel>, query: &str, budget: Duration) -> Outcome {
    let mut request = Request::new(FindRequest { query: query.to_string() });
    request.set_timeout(budget);
    match client.find(request).await {
        Ok(reply) => Outcome::Hits(reply.into_inner().hits),
        Err(status) if matches!(status.code(), Code::DeadlineExceeded | Code::Cancelled) => Outcome::TimedOut,
        Err(status) => Outcome::Failed(status),
    }
}
