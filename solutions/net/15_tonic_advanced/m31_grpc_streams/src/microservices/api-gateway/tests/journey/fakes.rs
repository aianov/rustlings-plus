use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use grpc_kit::budget_of;
use proto::social::v1::auth_server::{Auth, AuthServer};
use proto::social::v1::chat_server::{Chat, ChatServer};
use proto::social::v1::users_server::{Users, UsersServer};
use proto::social::v1::{
    Account, Avatar, AvatarRequest, Changed, CreateProfileRequest, DeleteAvatarRequest,
    FindProfileRequest, GetAccountRequest, GetProfileRequest, ListProfilesRequest, PairRequest,
    Profile, ProfileList, PutAvatarReply, PutAvatarRequest, RelationsReply, RelationsRequest,
    ValidateTokenRequest,
};
use proto::social::v1::{HistoryReply, HistoryRequest, Message, SendRequest, SubscribeRequest};
use proto::{domain, request_id};
use service_runtime::CancellationToken;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::support::listener;

pub const LEAK: &str = "пароль hunter2 от базы profiles";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub method: String,
    pub request_id: String,
    pub user: Option<String>,
    pub budget: Option<Duration>,
}

#[derive(Clone, Default)]
pub struct Calls {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Calls {
    fn record<T>(&self, method: &str, request: &Request<T>) {
        let user = request
            .metadata()
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let seen = Seen {
            method: method.to_string(),
            request_id: request_id(request.metadata()),
            user,
            budget: budget_of(request),
        };
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(seen);
    }

    pub fn take(&self) -> Vec<(String, String)> {
        self.take_seen()
            .into_iter()
            .map(|seen| (seen.method, seen.request_id))
            .collect()
    }

    pub fn take_seen(&self) -> Vec<Seen> {
        std::mem::take(&mut *self.seen.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

pub struct FakeUsers {
    pub calls: Calls,
    pub delay: Duration,
}

const NAMES: [&str; 2] = ["alice", "bob"];

fn profile(id: u64) -> Option<Profile> {
    let name = NAMES.get(usize::try_from(id).ok()?.checked_sub(1)?)?;
    Some(Profile {
        id,
        username: name.to_string(),
    })
}

#[tonic::async_trait]
impl Users for FakeUsers {
    async fn create_profile(
        &self,
        request: Request<CreateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("CreateProfile", &request);
        Err(Status::unimplemented("фейк не регистрирует"))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("GetProfile", &request);
        tokio::time::sleep(self.delay).await;
        let id = request.into_inner().user_id;
        profile(id)
            .map(Response::new)
            .ok_or_else(|| domain("not_found", &format!("пользователь {id} не найден")))
    }

    async fn find_profile(
        &self,
        request: Request<FindProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("FindProfile", &request);
        let name = request.into_inner().username;
        if name == "leaky" {
            return Err(Status::internal(LEAK));
        }
        (1..=2)
            .filter_map(profile)
            .find(|profile| profile.username == name)
            .map(Response::new)
            .ok_or_else(|| domain("not_found", &format!("пользователя @{name} нет")))
    }

    async fn list_profiles(
        &self,
        request: Request<ListProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.calls.record("ListProfiles", &request);
        let after = request.into_inner().after;
        let profiles = (after + 1..=2).filter_map(profile).collect();
        Ok(Response::new(ProfileList { profiles }))
    }

    async fn follow(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        self.calls.record("Follow", &request);
        Ok(Response::new(Changed { changed: true }))
    }

    async fn mute(&self, request: Request<PairRequest>) -> Result<Response<Changed>, Status> {
        self.calls.record("Mute", &request);
        Ok(Response::new(Changed { changed: true }))
    }

    async fn relations(
        &self,
        request: Request<RelationsRequest>,
    ) -> Result<Response<RelationsReply>, Status> {
        self.calls.record("Relations", &request);
        let reply = match request.into_inner().user_id {
            1 => RelationsReply {
                followers: vec![2],
                audience: vec![2],
                ..RelationsReply::default()
            },
            _ => RelationsReply::default(),
        };
        Ok(Response::new(reply))
    }

    async fn put_avatar(
        &self,
        request: Request<PutAvatarRequest>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        self.calls.record("PutAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }

    async fn get_avatar(
        &self,
        request: Request<AvatarRequest>,
    ) -> Result<Response<Avatar>, Status> {
        self.calls.record("GetAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }

    async fn delete_avatar(
        &self,
        request: Request<DeleteAvatarRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.calls.record("DeleteAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }
}

pub struct FakeAuth;

#[tonic::async_trait]
impl Auth for FakeAuth {
    async fn validate_token(
        &self,
        request: Request<ValidateTokenRequest>,
    ) -> Result<Response<Account>, Status> {
        let token = request.into_inner().token;
        let id = match token.as_str() {
            "alice-token" => 1,
            "bob-token" => 2,
            _ => return Err(domain("unauthorized", "токен неизвестен")),
        };
        let account = profile(id).map(|profile| Account {
            user_id: profile.id,
            username: profile.username,
        });
        account
            .map(Response::new)
            .ok_or_else(|| Status::internal("фейк сломан"))
    }

    async fn get_account(
        &self,
        request: Request<GetAccountRequest>,
    ) -> Result<Response<Account>, Status> {
        let id = request.into_inner().user_id;
        let account = profile(id).map(|profile| Account {
            user_id: profile.id,
            username: profile.username,
        });
        account
            .map(Response::new)
            .ok_or_else(|| domain("not_found", "нет такой учётной записи"))
    }
}

pub async fn fake_users(stop: &CancellationToken) -> (SocketAddr, Calls) {
    slow_users(stop, Duration::ZERO).await
}

pub async fn slow_users(stop: &CancellationToken, delay: Duration) -> (SocketAddr, Calls) {
    let (listener, addr) = listener().await;
    let calls = Calls::default();
    let users = UsersServer::new(FakeUsers {
        calls: calls.clone(),
        delay,
    });
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(users)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    (addr, calls)
}

pub async fn fake_auth(stop: &CancellationToken) -> SocketAddr {
    let (listener, addr) = listener().await;
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(AuthServer::new(FakeAuth))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    addr
}

pub struct FakeChat {
    open: Arc<watch::Sender<usize>>,
}

#[tonic::async_trait]
impl Chat for FakeChat {
    async fn send(&self, _: Request<SendRequest>) -> Result<Response<Message>, Status> {
        Err(Status::unimplemented("фейк только считает подписки"))
    }

    async fn history(&self, _: Request<HistoryRequest>) -> Result<Response<HistoryReply>, Status> {
        Err(Status::unimplemented("фейк только считает подписки"))
    }

    type SubscribeStream = ReceiverStream<Result<Message, Status>>;

    async fn subscribe(
        &self,
        _: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let (tx, rx) = mpsc::channel(1);
        self.open.send_modify(|open| *open += 1);
        let open = Arc::clone(&self.open);
        tokio::spawn(async move {
            tx.closed().await;
            open.send_modify(|open| *open -= 1);
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub async fn fake_chat(stop: &CancellationToken) -> (SocketAddr, watch::Receiver<usize>) {
    let (listener, addr) = listener().await;
    let (open, seen) = watch::channel(0);
    let chat = ChatServer::new(FakeChat {
        open: Arc::new(open),
    });
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(chat)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    (addr, seen)
}
