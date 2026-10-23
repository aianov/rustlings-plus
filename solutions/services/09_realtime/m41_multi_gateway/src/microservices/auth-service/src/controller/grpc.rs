use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode};
use proto::social::v1::auth_server::Auth;
use proto::social::v1::{Account as AccountReply, GetAccountRequest, ValidateTokenRequest};
use proto::to_status;
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
        let principal = self
            .service
            .validate(&token)
            .await
            .map_err(|error| to_status(&error.body()))?;
        Ok(Response::new(AccountReply {
            user_id: principal.user_id,
            username: principal.username,
            role: principal.role.as_str().to_string(),
            expires_at: principal.expires,
            session: principal.session,
        }))
    }

    async fn get_account(
        &self,
        request: Request<GetAccountRequest>,
    ) -> Result<Response<AccountReply>, Status> {
        let id = request.into_inner().user_id;
        match self.service.account(id).await {
            Some(account) => Ok(Response::new(dto::reply(account))),
            None => Err(to_status(&ErrorBody::new(
                ErrorCode::NotFound,
                format!("учётной записи {id} нет"),
            ))),
        }
    }
}
