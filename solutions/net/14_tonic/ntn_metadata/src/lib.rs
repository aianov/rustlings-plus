// Сквозной x-request-id и бинарная трасса: сервер возвращает их в ответе, клиент кладёт в запрос.
// Заголовки приходят из сети, поэтому доверять им нельзя ни в одну сторону.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::users_server::{Users, UsersServer};
use pb::{CreateUserRequest, GetUserRequest, User};

#[derive(Default)]
pub struct UsersSvc {
    generated: AtomicU64,
}

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let echoed = request.metadata().get("x-request-id").and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()).map(str::to_string);
        let request_id = echoed.unwrap_or_else(|| format!("req-{}", self.generated.fetch_add(1, Ordering::SeqCst) + 1));
        let trace = request.metadata().get_bin("trace-bin").and_then(|v| v.to_bytes().ok());
        let id = request.into_inner().id;
        let mut response = Response::new(User { id, handle: format!("user{id}"), display_name: String::new() });
        response.metadata_mut().insert("x-request-id", MetadataValue::try_from(request_id.as_str()).map_err(|_| Status::internal("id"))?);
        if let Some(trace) = trace {
            response.metadata_mut().insert_bin("trace-bin", MetadataValue::from_bytes(&trace));
        }
        Ok(response)
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("здесь не нужен"))
    }
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(UsersServer::new(UsersSvc::default())).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub user: User,
    pub request_id: String,
    pub trace: Option<Vec<u8>>,
}

pub async fn call_with(client: &mut UsersClient<Channel>, id: u64, request_id: &str, trace: Option<&[u8]>) -> Result<Reply, Status> {
    let printable = request_id.bytes().all(|b| (0x20..=0x7e).contains(&b));
    if request_id.is_empty() || request_id.len() > 64 || !printable {
        return Err(Status::invalid_argument("request_id: от 1 до 64 печатных ASCII-символов"));
    }
    let mut request = Request::new(GetUserRequest { id });
    request.metadata_mut().insert("x-request-id", MetadataValue::try_from(request_id).map_err(|_| Status::invalid_argument("request_id"))?);
    if let Some(trace) = trace {
        request.metadata_mut().insert_bin("trace-bin", MetadataValue::from_bytes(trace));
    }
    let response = client.get_user(request).await?;
    let request_id = response.metadata().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let trace = response.metadata().get_bin("trace-bin").and_then(|v| v.to_bytes().ok()).map(|b| b.to_vec());
    Ok(Reply { user: response.into_inner(), request_id, trace })
}
