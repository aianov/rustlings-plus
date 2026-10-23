use contracts::UserId;
use grpc_kit::caller_of;
use proto::request_id;
use proto::social::v1::{Avatar as AvatarReply, AvatarChunk, FileRequest, PutAvatarReply};
use tonic::{Request, Response, Status, Streaming};

use super::{UsersController, failed};

impl UsersController {
    pub(super) async fn upload_of(
        &self,
        request: Request<Streaming<AvatarChunk>>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let owner = UserId(caller_of(&request)?);
        let stream = request.into_inner();
        let chunks: files::Chunks = Box::pin(futures::StreamExt::map(stream, |chunk| {
            chunk
                .map(|chunk| chunk.data)
                .map_err(|status| std::io::Error::other(status.message().to_string()))
        }));
        let stored = self.service.put_avatar(owner, chunks, None).await;
        let (avatar, created) = stored.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(PutAvatarReply {
            created,
            key: avatar.key,
            content_type: avatar.content_type,
        }))
    }

    pub(super) async fn file_of(
        &self,
        request: Request<FileRequest>,
    ) -> Result<Response<AvatarReply>, Status> {
        let rid = request_id(request.metadata());
        let key = request.into_inner().key;
        let data = self.service.file(&key).await;
        let data = data.map_err(|error| failed(&error, &rid))?;
        let content_type = files::sniff(&data)
            .map_or("application/octet-stream", |kind| kind.mime())
            .to_string();
        Ok(Response::new(AvatarReply {
            content_type,
            data,
            key,
        }))
    }
}
