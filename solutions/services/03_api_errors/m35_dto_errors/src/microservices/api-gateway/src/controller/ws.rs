use std::sync::Arc;
use std::time::Duration;

use contracts::ErrorCode;
use grpc_kit::Stamp;
use http_engine::{Request, Response};
use websocket_engine::{WsConfig, check_upgrade, upgrade};

use crate::bootstrap::Shared;
use crate::controller::http::{fail, failure};

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
            ErrorCode::Unauthorized,
            "сокет открывается только с токеном: ?token= или Authorization: Bearer из POST /auth/login в auth-service",
        );
    };
    let checking = Stamp::fresh().within(shared.config.budget);
    let user = match shared.auth.check(&token, &checking).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return fail(
                ErrorCode::Unauthorized,
                "такого токена нет, получи свежий в auth-service: POST /auth/login",
            );
        }
        Err(body) => return failure(body),
    };
    let Ok(permit) = Arc::clone(&shared.sessions).acquire_owned().await else {
        return fail(ErrorCode::Unavailable, "шлюз останавливается");
    };
    let inbox = shared.inboxes.join(user.id).await;
    let config = WsConfig {
        allowed_origins: shared.config.cors_origins.clone(),
        allow_missing_origin: true,
        max_message: shared.config.max_line,
        close_timeout: Duration::from_secs(1),
    };
    upgrade(&request, &config, move |ws| {
        super::conn::serve(ws, shared, user, permit, inbox)
    })
}
