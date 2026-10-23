use std::pin::Pin;
use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::chat_server::Chat;
use proto::social::v1::{
    HistoryReply, HistoryRequest, Message as MessageReply, SendRequest, SubscribeRequest,
};
use proto::{internal, invalid, to_status};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};
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
        eprintln!("chat-service [{rid}]: {}", service_runtime::report(error));
    }
    to_status(&ErrorBody::new(error.code(), error.to_string()))
}

impl ChatController {
    pub fn new(service: Arc<ChatService>, users: Channel) -> ChatController {
        ChatController { service, users }
    }

    fn users<T>(&self, request: &Request<T>) -> (GrpcUsers, String) {
        let stamp = Stamp::follow(request);
        let rid = stamp.request_id().to_string();
        (GrpcUsers::new(self.users.clone(), stamp), rid)
    }
}

type Messages = Pin<Box<dyn Stream<Item = Result<MessageReply, Status>> + Send>>;

#[tonic::async_trait]
impl Chat for ChatController {
    async fn send(&self, request: Request<SendRequest>) -> Result<Response<MessageReply>, Status> {
        let from = UserId(caller_of(&request)?);
        let (users, rid) = self.users(&request);
        let text = dto::text(request.get_ref()).map_err(|errors| invalid(&errors))?;
        let service = Arc::clone(&self.service);
        let to = UserId(request.into_inner().peer_id);
        let job = tokio::spawn(async move { service.send(&users, from, to, text).await });
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
        let (users, rid) = self.users(&request);
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

    type SubscribeStream = Messages;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let user = UserId(caller_of(&request)?);
        let inbox = self.service.subscribe(user);
        let inbox = inbox.map_err(|error| failed(&error, "-"))?;
        let stream = ReceiverStream::new(inbox).map(|message| Ok(dto::message(&message)));
        Ok(Response::new(Box::pin(stream)))
    }
}
