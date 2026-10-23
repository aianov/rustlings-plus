mod auth;
mod notify;
mod upstream;

use contracts::{ErrorBody, ErrorCode};
use grpc_kit::{Readiness, Stamp, Tls, Upstream};
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::feeds_client::FeedsClient;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{auth_server, chat_server, feeds_server, posts_server, users_server};
use proto::{MESSAGE_MAX, from_status};
use service_runtime::Background;
use tonic::Status;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;

use crate::common::{Error, Services};

pub use auth::RemoteAuth;
pub use notify::Notify;
pub use upstream::{AuthHttp, Relayed};

pub type Stamped = InterceptedService<Channel, Stamp>;

#[derive(Clone)]
pub struct Upstreams {
    auth: Channel,
    users: Channel,
    posts: Channel,
    chat: Channel,
    feed: Channel,
}

fn channel(service: &'static str, addr: &str, tls: Option<&Tls>) -> Result<Channel, Error> {
    grpc_kit::channel(addr, service, tls).map_err(|source| Error::Upstream { service, source })
}

impl Upstreams {
    pub fn connect(services: &Services, tls: Option<&Tls>) -> Result<Upstreams, Error> {
        Ok(Upstreams {
            auth: channel("auth-service", &services.auth, tls)?,
            users: channel("user-service", &services.users, tls)?,
            posts: channel("post-service", &services.posts, tls)?,
            chat: channel("chat-service", &services.chat, tls)?,
            feed: channel("feed-service", &services.feed, tls)?,
        })
    }

    pub fn readiness(&self, background: &mut Background) -> Readiness {
        let upstream = |label, service, channel: &Channel| Upstream {
            label,
            service,
            channel: channel.clone(),
        };
        Readiness::supervised(
            vec![
                upstream("auth-service", auth_server::SERVICE_NAME, &self.auth),
                upstream("user-service", users_server::SERVICE_NAME, &self.users),
                upstream("post-service", posts_server::SERVICE_NAME, &self.posts),
                upstream("chat-service", chat_server::SERVICE_NAME, &self.chat),
                upstream("feed-service", feeds_server::SERVICE_NAME, &self.feed),
            ],
            background,
        )
    }

    pub fn auth(&self, stamp: &Stamp) -> AuthClient<Stamped> {
        AuthClient::with_interceptor(self.auth.clone(), stamp.clone())
    }

    pub fn users(&self, stamp: &Stamp) -> UsersClient<Stamped> {
        UsersClient::with_interceptor(self.users.clone(), stamp.clone())
            .max_decoding_message_size(MESSAGE_MAX)
            .max_encoding_message_size(MESSAGE_MAX)
    }

    pub fn posts(&self, stamp: &Stamp) -> PostsClient<Stamped> {
        PostsClient::with_interceptor(self.posts.clone(), stamp.clone())
    }

    pub fn chat(&self, stamp: &Stamp) -> ChatClient<Stamped> {
        ChatClient::with_interceptor(self.chat.clone(), stamp.clone())
    }

    pub fn feed(&self, stamp: &Stamp) -> FeedsClient<Stamped> {
        FeedsClient::with_interceptor(self.feed.clone(), stamp.clone())
            .max_decoding_message_size(MESSAGE_MAX)
    }
}

pub(crate) fn failure(status: &Status, service: &str, rid: &str) -> ErrorBody {
    let body = from_status(status, service);
    if !body.known().is_some_and(ErrorCode::is_client_fault) {
        eprintln!(
            "api-gateway [{rid}]: {service} ответил {:?}: {}",
            status.code(),
            status.message()
        );
    }
    body.with_request_id(rid)
}
