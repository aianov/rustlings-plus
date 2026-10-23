use contracts::ErrorBody;
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::{MESSAGE_MAX, explain};
use tonic::Status;
use tonic::transport::{Channel, Endpoint};

use crate::config::Services;
use crate::error::Error;

#[derive(Clone)]
pub struct Upstreams {
    auth: Channel,
    users: Channel,
    posts: Channel,
}

fn channel(service: &'static str, addr: &str) -> Result<Channel, Error> {
    let endpoint = Endpoint::from_shared(format!("http://{addr}"))
        .map_err(|source| Error::Upstream { service, source })?;
    Ok(endpoint.connect_lazy())
}

impl Upstreams {
    pub fn connect(services: &Services) -> Result<Upstreams, Error> {
        Ok(Upstreams {
            auth: channel("auth-service", &services.auth)?,
            users: channel("user-service", &services.users)?,
            posts: channel("post-service", &services.posts)?,
        })
    }

    pub fn auth(&self) -> AuthClient<Channel> {
        AuthClient::new(self.auth.clone())
    }

    pub fn users(&self) -> UsersClient<Channel> {
        UsersClient::new(self.users.clone())
            .max_decoding_message_size(MESSAGE_MAX)
            .max_encoding_message_size(MESSAGE_MAX)
    }

    pub fn posts(&self) -> PostsClient<Channel> {
        PostsClient::new(self.posts.clone())
    }
}

pub(crate) fn failure(status: &Status, service: &str, rid: &str) -> ErrorBody {
    let (code, message) = explain(status, service);
    if code == "internal" {
        eprintln!("api-gateway [{rid}]: {service} ответил {status}");
    }
    ErrorBody { code, message }
}
