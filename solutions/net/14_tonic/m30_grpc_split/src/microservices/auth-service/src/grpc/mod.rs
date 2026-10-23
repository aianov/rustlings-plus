use std::sync::Arc;

use proto::domain;
use proto::social::v1::auth_server::Auth;
use proto::social::v1::{Account as AccountReply, GetAccountRequest, ValidateTokenRequest};
use tonic::{Request, Response, Status};

use crate::accounts::{Account, Accounts};
use crate::tokens::Tokens;

pub struct AuthService {
    accounts: Arc<Accounts>,
    tokens: Arc<Tokens>,
}

impl AuthService {
    pub fn new(accounts: Arc<Accounts>, tokens: Arc<Tokens>) -> AuthService {
        AuthService { accounts, tokens }
    }
}

fn reply(account: Account) -> Response<AccountReply> {
    Response::new(AccountReply {
        user_id: account.user_id,
        username: account.username,
    })
}

#[tonic::async_trait]
impl Auth for AuthService {
    async fn validate_token(
        &self,
        request: Request<ValidateTokenRequest>,
    ) -> Result<Response<AccountReply>, Status> {
        let token = request.into_inner().token;
        match self.tokens.account(&token) {
            Some(account) => Ok(reply(account)),
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
        match self.accounts.get(id) {
            Some(account) => Ok(reply(account)),
            None => Err(domain("not_found", &format!("учётной записи {id} нет"))),
        }
    }
}
