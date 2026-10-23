use futures::future::BoxFuture;
use serde_json::json;
use std::time::Duration;

pub struct ProxyRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub peer_ip: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum UpstreamError {
    Connect,
    Reset,
}

pub trait Upstream: Send + Sync {
    fn send(&self, request: ProxyRequest) -> BoxFuture<'_, Result<ProxyResponse, UpstreamError>>;
}

pub struct ProxyConfig {
    pub max_body: usize,
    pub timeout: Duration,
}

pub async fn proxy(request: ProxyRequest, upstream: &dyn Upstream, config: &ProxyConfig) -> ProxyResponse {
    todo!()
}
