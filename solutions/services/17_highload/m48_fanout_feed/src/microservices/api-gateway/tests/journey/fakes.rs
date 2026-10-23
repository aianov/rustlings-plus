use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use grpc_kit::budget_of;
use proto::social::v1::auth_server::{Auth, AuthServer};
use proto::social::v1::chat_server::{Chat, ChatServer};
use proto::social::v1::posts_server::{Posts, PostsServer};
use proto::social::v1::users_server::{Users, UsersServer};
use proto::social::v1::{
    Account, Avatar, AvatarChunk, AvatarRequest, Changed, CreateProfileRequest,
    DeleteAvatarRequest, FileRequest, FindProfileRequest, GetAccountRequest, GetProfileRequest,
    ListProfilesRequest, OwnProfileRequest, PairRequest, Profile, ProfileList, PutAvatarReply,
    RelationsReply, RelationsRequest, UpdateProfileRequest, ValidateTokenRequest,
};
use proto::social::v1::{
    AuthorPostsRequest, CommentReply, CommentRequest, CreatePostRequest, EditPostRequest,
    FeedEvent, FeedReply, FeedRequest, GetPostRequest, LikeReply, Post, PostAction, TrendsReply,
    TrendsRequest, WatchRequest,
};
use proto::social::v1::{
    HistoryReply, HistoryRequest, Message, SendRequest, TypingReply, TypingRequest,
};
use proto::{domain, request_id};
use service_runtime::CancellationToken;
use tokio::sync::watch;
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
    pub role: Option<String>,
    pub passed: Vec<(String, String)>,
    pub internal: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Calls {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Calls {
    fn record<T>(&self, method: &str, request: &Request<T>) {
        let text = |key: &str| {
            request
                .metadata()
                .get(key)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let passed = ["x-trace-id", "accept-language"]
            .into_iter()
            .filter_map(|key| text(key).map(|value| (key.to_string(), value)))
            .collect();
        let internal = request
            .metadata()
            .keys()
            .filter_map(|key| match key {
                tonic::metadata::KeyRef::Ascii(key) => Some(key.as_str().to_string()),
                tonic::metadata::KeyRef::Binary(..) => None,
            })
            .filter(|key| key.starts_with("x-internal-"))
            .collect();
        let seen = Seen {
            method: method.to_string(),
            request_id: request_id(request.metadata()),
            user: text("x-user-id"),
            budget: budget_of(request),
            role: text("x-user-role"),
            passed,
            internal,
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
        ..Default::default()
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

    async fn update_profile(
        &self,
        request: Request<UpdateProfileRequest>,
    ) -> Result<Response<Profile>, Status> {
        self.calls.record("UpdateProfile", &request);
        Err(Status::unimplemented("фейк не меняет профили"))
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

    async fn upload_avatar(
        &self,
        request: Request<tonic::Streaming<AvatarChunk>>,
    ) -> Result<Response<PutAvatarReply>, Status> {
        self.calls.record("UploadAvatar", &request);
        Err(Status::unimplemented("фейк без аватаров"))
    }

    async fn get_profiles(
        &self,
        request: Request<proto::social::v1::ProfilesRequest>,
    ) -> Result<Response<ProfileList>, Status> {
        self.calls.record("GetProfiles", &request);
        Ok(Response::new(ProfileList::default()))
    }

    async fn get_file(&self, request: Request<FileRequest>) -> Result<Response<Avatar>, Status> {
        self.calls.record("GetFile", &request);
        Err(Status::unimplemented("фейк без файлов"))
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

    async fn hide_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.calls.record("HideProfile", &request);
        Err(Status::unimplemented("фейк без саги удаления"))
    }

    async fn show_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.calls.record("ShowProfile", &request);
        Err(Status::unimplemented("фейк без саги удаления"))
    }

    async fn delete_profile(
        &self,
        request: Request<OwnProfileRequest>,
    ) -> Result<Response<Changed>, Status> {
        self.calls.record("DeleteProfile", &request);
        Err(Status::unimplemented("фейк без саги удаления"))
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
            role: String::from("user"),
            expires_at: api_gateway::unix_now() + 3600,
            session: format!("fake-{id}"),
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
            ..Account::default()
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
    typed: Arc<watch::Sender<usize>>,
}

#[tonic::async_trait]
impl Chat for FakeChat {
    async fn send(&self, _: Request<SendRequest>) -> Result<Response<Message>, Status> {
        Err(Status::unimplemented("фейк только считает «печатает»"))
    }

    async fn history(&self, _: Request<HistoryRequest>) -> Result<Response<HistoryReply>, Status> {
        Err(Status::unimplemented("фейк только считает «печатает»"))
    }

    async fn typing(&self, _: Request<TypingRequest>) -> Result<Response<TypingReply>, Status> {
        self.typed.send_modify(|typed| *typed += 1);
        Ok(Response::new(TypingReply { sent: true }))
    }
}

pub async fn fake_chat(stop: &CancellationToken) -> (SocketAddr, watch::Receiver<usize>) {
    let (listener, addr) = listener().await;
    let (typed, seen) = watch::channel(0);
    let chat = ChatServer::new(FakeChat {
        typed: Arc::new(typed),
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

pub struct FakePosts {
    pub calls: Calls,
    pub delay: Duration,
}

fn post(id: u64, author_id: u64) -> Post {
    Post {
        id,
        author_id,
        author_name: NAMES[0].to_string(),
        text: format!("пост {id}"),
        ..Post::default()
    }
}

fn only_profiles() -> Status {
    Status::unimplemented("фейк постов умеет только посты автора")
}

#[tonic::async_trait]
impl Posts for FakePosts {
    async fn create_post(&self, _: Request<CreatePostRequest>) -> Result<Response<Post>, Status> {
        Err(only_profiles())
    }

    async fn get_post(&self, _: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        Err(only_profiles())
    }

    async fn like(&self, _: Request<PostAction>) -> Result<Response<LikeReply>, Status> {
        Err(only_profiles())
    }

    async fn edit_post(&self, _: Request<EditPostRequest>) -> Result<Response<Post>, Status> {
        Err(only_profiles())
    }

    async fn delete_post(&self, _: Request<PostAction>) -> Result<Response<Changed>, Status> {
        Err(only_profiles())
    }

    async fn feed(&self, _: Request<FeedRequest>) -> Result<Response<FeedReply>, Status> {
        Err(only_profiles())
    }

    async fn author_posts(
        &self,
        request: Request<AuthorPostsRequest>,
    ) -> Result<Response<FeedReply>, Status> {
        self.calls.record("AuthorPosts", &request);
        tokio::time::sleep(self.delay).await;
        let author = request.into_inner().author_id;
        let posts = vec![post(2, author), post(1, author)];
        Ok(Response::new(FeedReply {
            posts,
            next: String::new(),
        }))
    }

    async fn comment(&self, _: Request<CommentRequest>) -> Result<Response<CommentReply>, Status> {
        Err(only_profiles())
    }

    async fn trends(&self, _: Request<TrendsRequest>) -> Result<Response<TrendsReply>, Status> {
        Err(only_profiles())
    }

    type WatchStream = ReceiverStream<Result<FeedEvent, Status>>;

    async fn watch(&self, _: Request<WatchRequest>) -> Result<Response<Self::WatchStream>, Status> {
        Err(only_profiles())
    }
}

pub async fn slow_posts(stop: &CancellationToken, delay: Duration) -> (SocketAddr, Calls) {
    let (listener, addr) = listener().await;
    let calls = Calls::default();
    let posts = PostsServer::new(FakePosts {
        calls: calls.clone(),
        delay,
    });
    let stop = stop.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(posts)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                stop.cancelled().await
            })
            .await
    });
    (addr, calls)
}
