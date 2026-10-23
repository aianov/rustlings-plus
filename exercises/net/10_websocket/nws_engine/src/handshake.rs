use std::future::Future;
use std::time::Duration;

use http_engine::{Method, Request, Response, Version};
use tokio::net::TcpStream;

use crate::base64;
use crate::frame::Role;
use crate::stream::WsStream;

#[derive(Debug, Clone)]
pub struct WsConfig {
    pub allowed_origins: Vec<String>,
    pub allow_missing_origin: bool,
    pub max_message: usize,
    pub close_timeout: Duration,
}

impl Default for WsConfig {
    fn default() -> WsConfig {
        WsConfig { allowed_origins: Vec::new(), allow_missing_origin: true, max_message: 1 << 20, close_timeout: Duration::from_secs(5) }
    }
}

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn accept_key(client_key: &str) -> String {
    todo!()
}


pub fn check_upgrade(request: &Request) -> Result<String, Response> {
    todo!()
}


pub fn upgrade<F, Fut>(request: &Request, config: &WsConfig, on_connect: F) -> Response
where
    F: FnOnce(WsStream<TcpStream>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    todo!()
}
