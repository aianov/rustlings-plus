mod conn;

use std::sync::Arc;
use std::time::Duration;

use http_engine::{Request, Response};
use proto::new_request_id;
use websocket_engine::{WsConfig, check_upgrade, upgrade};

use crate::http::{fail, failure};
use crate::state::Shared;

pub const GOING_AWAY: u16 = 1001;

pub const UNSUPPORTED: u16 = 1003;

fn token_of(request: &Request) -> Option<String> {
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim().to_string());
    let token = bearer.or_else(|| {
        request
            .query()
            .ok()?
            .into_iter()
            .find(|(name, _)| name == "token")
            .map(|(_, value)| value)
    });
    token.filter(|token| !token.is_empty())
}

pub(crate) async fn open(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = check_upgrade(&request) {
        return response;
    }
    let Some(token) = token_of(&request) else {
        return fail(
            "unauthorized",
            "сокет открывается только с токеном: ?token= или Authorization: Bearer из POST /auth/login в auth-service",
        );
    };
    let user = match shared.auth.check(&token, &new_request_id()).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return fail(
                "unauthorized",
                "такого токена нет, получи свежий в auth-service: POST /auth/login",
            );
        }
        Err(body) => return failure(body),
    };
    let Ok(permit) = Arc::clone(&shared.sessions).acquire_owned().await else {
        return fail("unavailable", "шлюз останавливается");
    };
    let config = WsConfig {
        allowed_origins: shared.config.cors_origins.clone(),
        allow_missing_origin: true,
        max_message: shared.config.max_line,
        close_timeout: Duration::from_secs(1),
    };
    upgrade(&request, &config, move |ws| {
        conn::serve(ws, shared, user, permit)
    })
}
