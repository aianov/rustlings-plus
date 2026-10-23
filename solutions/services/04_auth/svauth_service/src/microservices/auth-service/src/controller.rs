use crate::common::{HttpReply, HttpRequest, failure, ok};
use crate::dto::{BodyError, parse_login, parse_refresh, parse_register};
use crate::service::{Account, AuthService, Principal, ServiceError, Tokens, ValidateError};
use serde_json::json;

pub struct AuthApp {
    service: AuthService,
}

fn bad_body(error: BodyError) -> HttpReply {
    match error {
        BodyError::Malformed => failure(400, "request.malformed", "тело запроса не JSON-объект", &[]),
        BodyError::Invalid(details) => failure(422, "validation.failed", "проверь поля", &details),
    }
}

fn service_error(error: ServiceError) -> HttpReply {
    match error {
        ServiceError::UsernameTaken => failure(409, "user.username_taken", "имя уже занято", &[]),
        ServiceError::InvalidCredentials => failure(401, "auth.invalid_credentials", "неверное имя или пароль", &[]),
        ServiceError::InvalidRefresh => failure(401, "auth.invalid_refresh", "войди заново", &[]),
        ServiceError::Unauthenticated(ValidateError::Expired) => failure(401, "auth.expired", "токен просрочен", &[]),
        ServiceError::Unauthenticated(_) => failure(401, "auth.unauthenticated", "нужно войти", &[]),
    }
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
        match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/register") => match parse_register(&request.body) {
                Ok(new) => match self.service.register(new).await {
                    Ok(account) => ok(201, json!({"id": account.id, "username": account.username})),
                    Err(error) => service_error(error),
                },
                Err(error) => bad_body(error),
            },
            ("POST", "/login") => match parse_login(&request.body) {
                Ok(credentials) => self.service.login(credentials).await.map(tokens_reply).unwrap_or_else(service_error),
                Err(error) => bad_body(error),
            },
            ("POST", "/refresh") => match parse_refresh(&request.body) {
                Ok(token) => self.service.refresh(&token).map(tokens_reply).unwrap_or_else(service_error),
                Err(error) => bad_body(error),
            },
            ("POST", "/logout") => self.authed(&request, |token| self.service.logout(token)),
            ("POST", "/logout-all") => self.authed(&request, |token| self.service.logout_all(token)),
            _ => failure(404, "not_found", "нет такого маршрута", &[]),
        }
    }

    fn authed(&self, request: &HttpRequest, action: impl FnOnce(&str) -> Result<(), ServiceError>) -> HttpReply {
        match bearer(request) {
            None => failure(401, "auth.unauthenticated", "нужно войти", &[]),
            Some(token) => match action(token) {
                Ok(()) => HttpReply { status: 204, body: String::new() },
                Err(error) => service_error(error),
            },
        }
    }

    pub fn validate_token(&self, token: &str) -> Result<Principal, ValidateError> {
        self.service.validate(token)
    }

    pub fn get_account(&self, user_id: &str) -> Option<Account> {
        self.service.account(user_id)
    }
}
