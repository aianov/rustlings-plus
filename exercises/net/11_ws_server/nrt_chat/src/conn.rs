use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::watch;
use websocket_engine::WsStream;

use crate::hub::Hub;
use crate::router::Router;
use crate::{Config, Stats};

pub struct Shared {
    pub hub: Hub,
    pub router: Router,
    pub config: Config,
    pub stats: Arc<Stats>,
}

pub async fn serve_connection(ws: WsStream<TcpStream>, user: String, shared: Arc<Shared>, stop: watch::Receiver<bool>) {
    let _ = (&ws, &user, &shared, &stop, Duration::ZERO);
    todo!()
}
