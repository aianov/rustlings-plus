use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Origin, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::chat_server::Chat;
use proto::social::v1::{
    HistoryReply, HistoryRequest, Message as MessageReply, SendRequest, TypingReply, TypingRequest,
};
use proto::{internal, invalid, to_status};
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::client::GrpcUsers;
use crate::dto;
use crate::service::{ChatError, ChatService};

pub const HISTORY_DEFAULT: u32 = 20;

pub const HISTORY_MAX: u32 = 100;

pub struct ChatController {
    service: Arc<ChatService>,
    users: Channel,
}

fn failed(error: &ChatError, rid: &str) -> Status {
    if error.code() == ErrorCode::Internal {
        tracing::warn!("chat-service [{rid}]: {}", service_runtime::report(error));
    }
    to_status(&ErrorBody::new(error.code(), error.to_string()))
}

impl ChatController {
    pub fn new(service: Arc<ChatService>, users: Channel) -> ChatController {
        ChatController { service, users }
    }

    fn users<T>(&self, request: &Request<T>) -> (GrpcUsers, Origin) {
        let stamp = Stamp::follow(request);
        let origin = Origin::new(stamp.request_id()).traced(stamp.trace_id());
        (GrpcUsers::new(self.users.clone(), stamp), origin)
    }
}

#[tonic::async_trait]
impl Chat for ChatController {
    async fn send(&self, request: Request<SendRequest>) -> Result<Response<MessageReply>, Status> {
        let from = UserId(caller_of(&request)?);
        let (users, origin) = self.users(&request);
        let text = dto::text(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let to = UserId(request.into_inner().peer_id);
        let rid = origin.request_id.clone();
        let job = tokio::spawn(async move { service.send(&users, from, to, text, &origin).await });
        match job.await {
            Ok(Ok(message)) => Ok(Response::new(dto::message(&message))),
            Ok(Err(error)) => Err(failed(&error, &rid)),
            Err(..) => Err(internal()),
        }
    }

    async fn history(
        &self,
        request: Request<HistoryRequest>,
    ) -> Result<Response<HistoryReply>, Status> {
        let user = UserId(caller_of(&request)?);
        let (users, origin) = self.users(&request);
        let rid = origin.request_id;
        let ask = request.into_inner();
        let limit = match ask.limit {
            0 => HISTORY_DEFAULT,
            limit => limit.min(HISTORY_MAX),
        };
        let found = self
            .service
            .history(&users, user, UserId(ask.peer_id), limit as usize)
            .await;
        let messages = found.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(HistoryReply {
            messages: messages.iter().map(dto::message).collect(),
        }))
    }

    async fn typing(
        &self,
        request: Request<TypingRequest>,
    ) -> Result<Response<TypingReply>, Status> {
        let from = UserId(caller_of(&request)?);
        let (users, origin) = self.users(&request);
        let to = UserId(request.into_inner().peer_id);
        let sent = self.service.typing(&users, from, to, &origin).await;
        let sent = sent.map_err(|error| failed(&error, &origin.request_id))?;
        Ok(Response::new(TypingReply { sent }))
    }
}
