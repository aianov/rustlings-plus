use std::future::Future;
use std::io;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::{Config, Stats, Tokens};
use std::time::Duration;

use http_engine::{Method, Request, Response, Router, ServerConfig, handler, serve_with};
use tokio::sync::watch;
use websocket_engine::{WsConfig, upgrade};

use crate::conn::{Shared, serve_connection};
use crate::hub::Hub;
use crate::methods;

fn token_of(request: &Request) -> Option<String> {
    if let Some(value) = request.headers.get("authorization") {
        if let Some((scheme, rest)) = value.split_once(' ') {
            if scheme.eq_ignore_ascii_case("bearer") {
                let token = rest.trim();
                return (!token.is_empty()).then(|| token.to_string());
            }
        }
    }
    let query = request.query().ok()?;
    query.into_iter().find(|(key, _)| key == "token").map(|(_, value)| value).filter(|value| !value.is_empty())
}

pub async fn run<F>(listener: TcpListener, tokens: Arc<dyn Tokens>, config: Config, stats: Arc<Stats>, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (stop_tx, stop_rx) = watch::channel(false);
    let drain = config.grace;
    let shared = Arc::new(Shared { hub: Hub::new(stats.clone()), router: methods::build(), config, stats });
    let ws_config = WsConfig { close_timeout: Duration::from_secs(1), ..WsConfig::default() };
    let ws_route = handler(move |request: Request| {
        let tokens = tokens.clone();
        let shared = shared.clone();
        let stop = stop_rx.clone();
        let ws_config = ws_config.clone();
        async move {
            let Some(token) = token_of(&request) else {
                return Response::text(401, "нужен токен");
            };
            let Some(user) = tokens.check(&token) else {
                return Response::text(401, "токен не подошёл");
            };
            upgrade(&request, &ws_config, move |ws| serve_connection(ws, user, shared, stop))
        }
    });
    let router = Router::new().route(Method::Get, "/ws", ws_route);
    let server = ServerConfig { drain_timeout: drain, ..ServerConfig::default() };
    let signal = async move {
        shutdown.await;
        let _ = stop_tx.send(true);
    };
    serve_with(listener, router.into_handler(), server, signal).await
}
