// Аватарки не проходят: tonic по умолчанию не принимает сообщения больше 4 МиБ. Лимит нужно поднять
// до 8 МиБ осознанно, на обеих сторонах и в обе стороны, а не снимать совсем.
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("files.v1");
}
use pb::files_client::FilesClient;
use pb::files_server::{Files, FilesServer};
use pb::{Blob, DownloadRequest, UploadReply};

pub const LIMIT: usize = 8 * 1024 * 1024;

pub struct FilesSvc;

#[tonic::async_trait]
impl Files for FilesSvc {
    async fn download(&self, request: Request<DownloadRequest>) -> Result<Response<Blob>, Status> {
        Ok(Response::new(Blob { data: vec![7; request.into_inner().size as usize] }))
    }

    async fn upload(&self, request: Request<Blob>) -> Result<Response<UploadReply>, Status> {
        Ok(Response::new(UploadReply { size: request.into_inner().data.len() as u64 }))
    }
}

pub fn service() -> FilesServer<FilesSvc> {
    FilesServer::new(FilesSvc).max_decoding_message_size(LIMIT).max_encoding_message_size(LIMIT)
}

pub fn client(channel: Channel) -> FilesClient<Channel> {
    FilesClient::new(channel).max_decoding_message_size(LIMIT).max_encoding_message_size(LIMIT)
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(service()).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
