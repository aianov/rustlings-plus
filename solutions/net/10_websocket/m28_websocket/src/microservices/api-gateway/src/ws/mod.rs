use std::sync::Arc;
use std::time::Duration;

use contracts::UserView;
use http_engine::{Request, Response};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::time::timeout;
use websocket_engine::{Message, WsConfig, WsStream, check_upgrade, upgrade};

use crate::http::fail;
use crate::protocol::{self, State};
use crate::state::Shared;

pub const GOING_AWAY: u16 = 1001;

pub const UNSUPPORTED: u16 = 1003;

fn token_of(request: &Request) -> Option<String> {
    let query = request.query().ok()?;
    query
        .into_iter()
        .find(|(name, _)| name == "token")
        .map(|(_, value)| value)
}

pub(crate) async fn open(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = check_upgrade(&request) {
        return response;
    }
    let user = match token_of(&request) {
        Some(token) => match shared.tokens.user(&token) {
            Some(user) => Some(user),
            None => {
                return fail(
                    "unauthorized",
                    "такого токена нет, получи свежий: POST /auth/login",
                );
            }
        },
        None => None,
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
        session(ws, shared, user, permit)
    })
}

async fn session(
    mut ws: WsStream<TcpStream>,
    shared: Arc<Shared>,
    user: Option<UserView>,
    permit: OwnedSemaphorePermit,
) {
    let _permit = permit;
    let (outbox, mut pushes) = mpsc::channel(shared.config.outbox.max(1));
    let mut state = State::new(outbox);
    if let Some(user) = user {
        state.introduce(&shared.hub, user);
    }
    let idle = shared.config.idle_timeout;
    loop {
        tokio::select! {
            biased;
            () = shared.shutdown.cancelled() => {
                let _ = ws.close(GOING_AWAY, "шлюз останавливается").await;
                break;
            }
            push = pushes.recv() => match push {
                Some(line) => {
                    if ws.send(Message::Text(line)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            message = timeout(idle, ws.recv()) => match message {
                Ok(Ok(Some(Message::Text(text)))) => {
                    if text.trim().is_empty() {
                        continue;
                    }
                    let reply = protocol::handle(&shared, &mut state, &text).await;
                    if ws.send(Message::Text(reply)).await.is_err() {
                        break;
                    }
                }
                Ok(Ok(Some(Message::Binary(..)))) => {
                    let _ = ws.close(UNSUPPORTED, "конверты только текстом").await;
                    break;
                }
                Ok(Ok(None) | Err(..)) => break,
                Err(..) => {
                    let _ = ws.close(GOING_AWAY, "простой").await;
                    break;
                }
            },
        }
    }
}
