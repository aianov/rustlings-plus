use std::sync::Arc;

use proto::domain;
use proto::social::v1::auth_server::Auth;
use proto::social::v1::{Account as AccountReply, GetAccountRequest, ValidateTokenRequest};
use tonic::{Request, Response, Status};

use crate::dto;
use crate::service::AuthService;

pub struct AuthController {
    service: Arc<AuthService>,
}

impl AuthController {
    pub fn new(service: Arc<AuthService>) -> AuthController {
        AuthController { service }
    }
}

#[tonic::async_trait]
impl Auth for AuthController {
    async fn validate_token(
        &self,
        request: Request<ValidateTokenRequest>,
    ) -> Result<Response<AccountReply>, Status> {
        let token = request.into_inner().token;
        match self.service.validate(&token).await {
            Some(account) => Ok(Response::new(dto::reply(account))),
            None => Err(domain(
                "unauthorized",
                "такого токена нет, получи свежий: POST /auth/login",
            )),
        }
    }

    async fn get_account(
        &self,
        request: Request<GetAccountRequest>,
    ) -> Result<Response<AccountReply>, Status> {
        let id = request.into_inner().user_id;
        match self.service.account(id).await {
            Some(account) => Ok(Response::new(dto::reply(account))),
            None => Err(domain("not_found", &format!("учётной записи {id} нет"))),
        }
    }
}
