// Токен в заголовок authorization нужен каждому вызову. Перехватчик клиента кладёт его сам.
// Допиши call: токен берётся в момент вызова, а не при создании клиента.
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
        if request.metadata().contains_key("authorization") {
            return Ok(request);
        }
        let token = self.token.read().unwrap().clone();
        if token.is_empty() {
            return Ok(request);
        }
        if token.bytes().any(|b| !(0x21..=0x7e).contains(&b)) {
            return Err(Status::unauthenticated("токен содержит недопустимые символы"));
        }
        let value = MetadataValue::try_from(format!("Bearer {token}")).map_err(|_| Status::unauthenticated("токен содержит недопустимые символы"))?;
        request.metadata_mut().insert("authorization", value);
        Ok(request)
    }
}

pub type AuthedClient = UsersClient<InterceptedService<Channel, TokenInterceptor>>;

pub fn authed_client(channel: Channel, token: SharedToken) -> AuthedClient {
    UsersClient::with_interceptor(channel, TokenInterceptor::new(token))
}
