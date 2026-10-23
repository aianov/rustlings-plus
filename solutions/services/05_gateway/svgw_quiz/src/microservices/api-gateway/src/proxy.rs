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

const HOP_BY_HOP: [&str; 8] = ["connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade"];
const UNTRUSTED: [&str; 5] = ["x-forwarded-for", "x-real-ip", "forwarded", "x-forwarded-proto", "x-forwarded-host"];

fn strip_hop_by_hop(headers: &[(String, String)]) -> Vec<(String, String)> {
    let named: Vec<String> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("connection"))
        .flat_map(|(_, value)| value.split(',').map(|token| token.trim().to_ascii_lowercase()).collect::<Vec<_>>())
        .collect();
    headers
        .iter()
        .filter(|(name, _)| {
            let lower = name.to_ascii_lowercase();
            !HOP_BY_HOP.contains(&lower.as_str()) && !named.contains(&lower)
        })
        .cloned()
        .collect()
}

fn failure(status: u16, code: &str) -> ProxyResponse {
    ProxyResponse {
        status,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: json!({"error": {"code": code}}).to_string().into_bytes(),
    }
}

pub async fn proxy(request: ProxyRequest, upstream: &dyn Upstream, config: &ProxyConfig) -> ProxyResponse {
    let Some(rest) = request.path.strip_prefix("/auth/") else {
        return failure(404, "not_found");
    };
    if request.body.len() > config.max_body {
        return failure(413, "payload_too_large");
    }
    let mut headers = strip_hop_by_hop(&request.headers);
    headers.retain(|(name, _)| !UNTRUSTED.contains(&name.to_ascii_lowercase().as_str()));
    headers.push(("x-forwarded-for".to_string(), request.peer_ip.clone()));
    headers.push(("x-real-ip".to_string(), request.peer_ip.clone()));
    headers.push(("x-forwarded-proto".to_string(), "https".to_string()));
    let forwarded = ProxyRequest { path: format!("/{rest}"), headers, ..request };
    match tokio::time::timeout(config.timeout, upstream.send(forwarded)).await {
        Err(_) => failure(504, "gateway_timeout"),
        Ok(Err(_)) => failure(502, "bad_gateway"),
        Ok(Ok(response)) => ProxyResponse { headers: strip_hop_by_hop(&response.headers), ..response },
    }
}
