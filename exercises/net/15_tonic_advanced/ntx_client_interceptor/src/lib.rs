// Токен в заголовок authorization нужен каждому вызову. Перехватчик клиента кладёт его сам.
// Допиши call: токен берётся в момент вызова, а не при создании клиента.
#![allow(unused_variables)]

use std::sync::{Arc, RwLock};
use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;
use tonic::{Request, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;

pub type SharedToken = Arc<RwLock<String>>;

#[derive(Clone)]
pub struct TokenInterceptor {
    token: SharedToken,
}

impl TokenInterceptor {
    pub fn new(token: SharedToken) -> Self {
        TokenInterceptor { token }
    }
}

impl Interceptor for TokenInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        todo!()
    }
}

pub type AuthedClient = UsersClient<InterceptedService<Channel, TokenInterceptor>>;

pub fn authed_client(channel: Channel, token: SharedToken) -> AuthedClient {
    UsersClient::with_interceptor(channel, TokenInterceptor::new(token))
}
