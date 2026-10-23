use std::sync::Arc;

use grpc_kit::{Stamp, caller_of};
use proto::social::v1::chat_server::Chat;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{
    GetProfileRequest, HistoryReply, HistoryRequest, Message, SendRequest, SubscribeRequest,
};
use proto::{domain, explain, internal};
use social_core::{Message as Stored, UserId, report};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::hub::Hub;
use crate::store::{Refused, Store};

pub const HISTORY_DEFAULT: u32 = 20;

pub const HISTORY_MAX: u32 = 100;

pub struct ChatService {
    store: Arc<Store>,
    hub: Arc<Hub>,
    users: Channel,
}

fn wire(message: &Stored) -> Message {
    Message {
        from_id: message.sender().0,
        to_id: message.recipient().0,
        text: message.text().to_string(),
    }
}

fn refused(refused: Refused, rid: &str) -> Status {
    match refused {
        Refused::Domain(error) => {
            let error = social_core::Error::from(error);
            domain(error.code(), &report(&error))
        }
        Refused::Journal(error) => {
            eprintln!("chat-service [{rid}]: журнал сообщений: {error}");
            internal()
        }
    }
}

impl ChatService {
    pub fn new(store: Arc<Store>, hub: Arc<Hub>, users: Channel) -> ChatService {
        ChatService { store, hub, users }
    }

    async fn known(&self, peer: u64, stamp: &Stamp) -> Result<(), Status> {
        let mut users = UsersClient::with_interceptor(self.users.clone(), stamp.clone());
        match users.get_profile(GetProfileRequest { user_id: peer }).await {
            Ok(..) => Ok(()),
            Err(status) => {
                let (code, message) = explain(&status, "сервис пользователей");
                if code == "internal" {
                    eprintln!(
                        "chat-service [{}]: user-service ответил {status}",
                        stamp.request_id()
                    );
                }
                Err(domain(&code, &message))
            }
        }
    }
}

#[tonic::async_trait]
impl Chat for ChatService {
    async fn send(&self, request: Request<SendRequest>) -> Result<Response<Message>, Status> {
        let from = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request);
        let ask = request.into_inner();
        self.known(ask.peer_id, &stamp).await?;
        let (store, hub) = (Arc::clone(&self.store), Arc::clone(&self.hub));
        let to = UserId(ask.peer_id);
        let job = tokio::spawn(async move {
            let stored = store.send(from, to, &ask.text).await?;
            let message = wire(&stored);
            hub.deliver(&message);
            Ok::<Message, Refused>(message)
        });
        match job.await {
            Ok(Ok(message)) => Ok(Response::new(message)),
            Ok(Err(why)) => Err(refused(why, stamp.request_id())),
            Err(..) => Err(internal()),
        }
    }

    async fn history(
        &self,
        request: Request<HistoryRequest>,
    ) -> Result<Response<HistoryReply>, Status> {
        let user = UserId(caller_of(&request)?);
        let stamp = Stamp::follow(&request);
        let ask = request.into_inner();
        self.known(ask.peer_id, &stamp).await?;
        let limit = match ask.limit {
            0 => HISTORY_DEFAULT,
            limit => limit.min(HISTORY_MAX),
        };
        let messages = self
            .store
            .history(user, UserId(ask.peer_id), limit as usize);
        Ok(Response::new(HistoryReply {
            messages: messages.iter().map(wire).collect(),
        }))
    }

    type SubscribeStream = ReceiverStream<Result<Message, Status>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let user = caller_of(&request)?;
        let inbox = self.hub.subscribe(user)?;
        Ok(Response::new(ReceiverStream::new(inbox)))
    }
}
