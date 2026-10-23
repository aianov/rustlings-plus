use crate::common::{HttpReply, HttpRequest, ok};
use crate::service::{Account, AuthService, Principal, Tokens, ValidateError};
use serde_json::json;

pub struct AuthApp {
    service: AuthService,
}

fn tokens_reply(tokens: Tokens) -> HttpReply {
    ok(200, json!({"access": tokens.access, "refresh": tokens.refresh, "expires_in": tokens.expires_in}))
}

fn bearer(request: &HttpRequest) -> Option<&str> {
    let value = request.header("authorization")?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() && !token.contains(' ')).then_some(token)
}

impl AuthApp {
    pub fn new(service: AuthService) -> AuthApp {
        AuthApp { service }
    }

    pub fn service(&self) -> &AuthService {
        &self.service
    }

    pub async fn handle(&self, request: HttpRequest) -> HttpReply {
        todo!()
    }

    pub fn validate_token(&self, token: &str) -> Result<Principal, ValidateError> {
        todo!()
    }

    pub fn get_account(&self, user_id: &str) -> Option<Account> {
        todo!()
    }
}
