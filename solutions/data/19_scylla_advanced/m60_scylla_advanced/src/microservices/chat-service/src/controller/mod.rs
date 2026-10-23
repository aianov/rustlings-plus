use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Origin, UserId};
use grpc_kit::{Stamp, caller_of};
use proto::social::v1::chat_server::Chat;
use proto::social::v1::{
    ChatSummary, ChatsReply, ChatsRequest, EditMessageRequest, EditReply, HistoryReply,
    HistoryRequest, Message as MessageReply, PeerRequest, SendRequest, TypingReply, TypingRequest,
};
use proto::{internal, invalid, to_status};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::client::GrpcUsers;
use crate::common::MessageText;
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
        let ask = request.into_inner();
        let to = UserId(ask.peer_id);
        let ttl = ask.ttl_seconds;
        let rid = origin.request_id.clone();
        let job = tokio::spawn(async move {
            match ttl {
                0 => service.send(&users, from, to, text, &origin).await,
                ttl => service.send_for(&users, from, to, text, ttl).await,
            }
        });
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
        let peer = UserId(ask.peer_id);
        if self.service.paged() {
            let page = self
                .service
                .page(&users, user, peer, ask.cursor, limit as usize)
                .await;
            let (messages, next) = page.map_err(|error| failed(&error, &rid))?;
            return Ok(Response::new(HistoryReply {
                messages: messages.iter().map(dto::message).collect(),
                next: next.unwrap_or_default(),
            }));
        }
        let found = self
            .service
            .history(&users, user, peer, limit as usize)
            .await;
        let messages = found.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(HistoryReply {
            messages: messages.iter().map(dto::message).collect(),
            next: String::new(),
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

    async fn chats(&self, request: Request<ChatsRequest>) -> Result<Response<ChatsReply>, Status> {
        let user = UserId(caller_of(&request)?);
        let rid = proto::request_id(request.metadata());
        let limit = request.into_inner().limit as usize;
        let chats = self.service.chats(user, limit).await;
        let chats = chats.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(ChatsReply {
            chats: chats
                .into_iter()
                .map(|chat| ChatSummary {
                    peer_id: chat.peer.0,
                    last_at: chat.last_at,
                    preview: chat.preview,
                    unread: chat.unread,
                })
                .collect(),
        }))
    }

    type ExportChatStream = ReceiverStream<Result<MessageReply, Status>>;

    async fn export_chat(
        &self,
        request: Request<HistoryRequest>,
    ) -> Result<Response<Self::ExportChatStream>, Status> {
        let user = UserId(caller_of(&request)?);
        let (users, origin) = self.users(&request);
        let peer = UserId(request.into_inner().peer_id);
        let exported = self.service.export(&users, user, peer).await;
        let mut messages = exported.map_err(|error| failed(&error, &origin.request_id))?;
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            while let Some(message) = messages.recv().await {
                if tx.send(Ok(dto::message(&message))).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn edit_message(
        &self,
        request: Request<EditMessageRequest>,
    ) -> Result<Response<EditReply>, Status> {
        let user = UserId(caller_of(&request)?);
        let rid = proto::request_id(request.metadata());
        let ask = request.into_inner();
        let text = MessageText::try_from(ask.text.as_str())
            .map_err(|violation| invalid(&validation::Invalid::single("text", violation)))?;
        let edited = self.service.edit(user, ask.message_id, text).await;
        edited.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(EditReply { changed: true }))
    }

    async fn mark_read(
        &self,
        request: Request<PeerRequest>,
    ) -> Result<Response<EditReply>, Status> {
        let user = UserId(caller_of(&request)?);
        let rid = proto::request_id(request.metadata());
        let peer = UserId(request.into_inner().peer_id);
        let marked = self.service.mark_read(user, peer).await;
        marked.map_err(|error| failed(&error, &rid))?;
        Ok(Response::new(EditReply { changed: true }))
    }
}
