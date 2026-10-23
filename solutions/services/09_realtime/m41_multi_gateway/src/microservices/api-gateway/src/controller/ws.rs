use std::sync::Arc;
use std::time::Duration;

use contracts::ErrorCode;
use grpc_kit::Stamp;
use http_engine::{Request, Response};
use websocket_engine::{WsConfig, check_upgrade, upgrade};

use super::conn::Opened;
use crate::bootstrap::Shared;
use crate::common::Visitor;
use crate::controller::http::{fail, failure};

pub const GOING_AWAY: u16 = 1001;

pub const UNSUPPORTED: u16 = 1003;

pub const EXPIRED: u16 = 4001;

fn header_token(request: &Request) -> Option<String> {
    request
        .headers
        .get("authorization")
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn query_tokens(request: &Request) -> Vec<String> {
    request
        .query()
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, value)| name == "token" && !value.is_empty())
        .map(|(_, value)| value)
        .collect()
}

fn after_of(request: &Request) -> Result<Option<u64>, Response> {
    let mut given = request
        .query()
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _)| name == "after");
    match (given.next(), given.next()) {
        (None, _) => Ok(None),
        (Some((_, value)), None) => value.parse().map(Some).map_err(|_| {
            fail(
                ErrorCode::BadData,
                "after это номер последнего увиденного сообщения: целое от 0",
            )
        }),
        (Some(..), Some(..)) => Err(fail(ErrorCode::BadData, "after указан дважды")),
    }
}

fn token_of(request: &Request) -> Result<Option<String>, Response> {
    let mut offered = query_tokens(request);
    offered.extend(header_token(request));
    offered.dedup();
    match offered.as_slice() {
        [] => Ok(None),
        [token] => Ok(Some(token.clone())),
        _ => Err(fail(
            ErrorCode::BadData,
            "токенов несколько: оставь один, заголовок Authorization или ?token=",
        )),
    }
}

pub(crate) async fn open(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = check_upgrade(&request) {
        return response;
    }
    let token = match token_of(&request) {
        Ok(token) => token,
        Err(response) => return response,
    };
    let visitor = match token {
        None => Visitor::Guest,
        Some(token) => {
            let checking = Stamp::fresh().within(shared.config.budget);
            match shared.auth.check(&token, &checking).await {
                Ok(Some(principal)) => Visitor::Member(principal),
                Ok(None) => {
                    return fail(
                        ErrorCode::Unauthorized,
                        "такого токена нет, получи свежий: POST /auth/login на этом же адресе",
                    );
                }
                Err(body) => return failure(body),
            }
        }
    };
    if let (Visitor::Member(principal), Some(realtime)) = (&visitor, &shared.realtime) {
        if realtime.is_revoked(&principal.session) {
            return fail(
                ErrorCode::Unauthorized,
                "сессия отозвана: войди снова, POST /auth/login",
            );
        }
    }
    let after = match after_of(&request) {
        Ok(after) => after,
        Err(response) => return response,
    };
    let Ok(permit) = Arc::clone(&shared.sessions).acquire_owned().await else {
        return fail(ErrorCode::Unavailable, "шлюз останавливается");
    };
    let (outbox, queue) = shared.registry.outbox(shared.config.outbox);
    let seat = match (&visitor, &shared.realtime) {
        (Visitor::Member(principal), Some(realtime)) => {
            Some(realtime.join(principal, outbox.clone(), after).await)
        }
        _ => None,
    };
    let opened = Opened {
        outbox,
        queue,
        seat,
        after,
    };
    let config = WsConfig {
        allowed_origins: shared.config.cors_origins.clone(),
        allow_missing_origin: true,
        max_message: shared.config.max_line,
        close_timeout: Duration::from_secs(1),
    };
    upgrade(&request, &config, move |ws| {
        super::conn::serve(ws, shared, visitor, permit, opened)
    })
}
