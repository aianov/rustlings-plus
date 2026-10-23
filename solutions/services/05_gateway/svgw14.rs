
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
    let headers = strip_hop_by_hop(&request.headers);
    let forwarded = ProxyRequest { path: format!("/{rest}"), headers, ..request };
    match tokio::time::timeout(config.timeout, upstream.send(forwarded)).await {
        Err(_) => failure(504, "gateway_timeout"),
        Ok(Err(_)) => failure(502, "bad_gateway"),
        Ok(Ok(response)) => ProxyResponse { headers: strip_hop_by_hop(&response.headers), ..response },
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::Mutex;

    struct Fake {
        seen: Mutex<Vec<ProxyRequest>>,
        answer: Mutex<Option<Result<ProxyResponse, UpstreamError>>>,
        delay: Duration,
    }

    impl Upstream for Fake {
        fn send(&self, request: ProxyRequest) -> BoxFuture<'_, Result<ProxyResponse, UpstreamError>> {
            async move {
                self.seen.lock().unwrap().push(request);
                tokio::time::sleep(self.delay).await;
                self.answer.lock().unwrap().take().unwrap_or(Ok(ProxyResponse { status: 200, headers: vec![], body: b"ok".to_vec() }))
            }
            .boxed()
        }
    }

    fn fake(answer: Option<Result<ProxyResponse, UpstreamError>>, delay_ms: u64) -> Fake {
        Fake { seen: Mutex::new(Vec::new()), answer: Mutex::new(answer), delay: Duration::from_millis(delay_ms) }
    }

    fn config() -> ProxyConfig {
        ProxyConfig { max_body: 1024, timeout: Duration::from_millis(500) }
    }

    fn request(path: &str, headers: &[(&str, &str)], body: usize) -> ProxyRequest {
        ProxyRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body: vec![b'x'; body],
            peer_ip: "203.0.113.9".to_string(),
        }
    }

    fn header<'a>(headers: &'a [(String, String)], name: &str) -> Vec<&'a str> {
        headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn path_is_rewritten_and_the_body_passes() {
        let up = fake(None, 1);
        let response = proxy(request("/auth/login", &[("content-type", "application/json")], 10), &up, &config()).await;
        assert_eq!((response.status, response.body.as_slice()), (200, b"ok".as_slice()), "ответ апстрима дошёл");
        let seen = up.seen.lock().unwrap();
        assert_eq!(seen[0].path, "/login", "префикс /auth убран");
        assert_eq!(seen[0].body.len(), 10, "тело передано целиком");
        assert_eq!(seen[0].method, "POST", "метод сохранён");
    }

    #[tokio::test(start_paused = true)]
    async fn foreign_paths_never_leave_the_gateway() {
        let up = fake(None, 1);
        for path in ["/users/1", "/auth", "/authx/login", "/"] {
            assert_eq!(proxy(request(path, &[], 0), &up, &config()).await.status, 404, "{path}");
        }
        assert!(up.seen.lock().unwrap().is_empty(), "апстрим не вызывался");
    }

    #[tokio::test(start_paused = true)]
    async fn hop_by_hop_headers_stay_on_their_hop_in_both_directions() {
        let answer = ProxyResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Connection".to_string(), "x-secret".to_string()),
                ("X-Secret".to_string(), "1".to_string()),
                ("Keep-Alive".to_string(), "timeout=5".to_string()),
                ("Transfer-Encoding".to_string(), "chunked".to_string()),
            ],
            body: vec![],
        };
        let up = fake(Some(Ok(answer)), 1);
        let headers = [
            ("Connection", "keep-alive, X-Trace"),
            ("X-Trace", "t"),
            ("Upgrade", "websocket"),
            ("TE", "trailers"),
            ("Proxy-Authorization", "Basic abc"),
            ("Authorization", "Bearer abc"),
            ("Content-Type", "application/json"),
        ];
        let response = proxy(request("/auth/login", &headers, 0), &up, &config()).await;
        let seen = up.seen.lock().unwrap();
        for gone in ["connection", "x-trace", "upgrade", "te", "proxy-authorization"] {
            assert!(header(&seen[0].headers, gone).is_empty(), "заголовок {gone} относится к соединению клиента и дальше не идёт");
        }
        assert_eq!(header(&seen[0].headers, "authorization"), vec!["Bearer abc"], "обычные заголовки остаются");
        assert_eq!(header(&seen[0].headers, "content-type"), vec!["application/json"], "и тип содержимого");
        for gone in ["connection", "x-secret", "keep-alive", "transfer-encoding"] {
            assert!(header(&response.headers, gone).is_empty(), "в ответе клиенту {gone} не должно быть");
        }
        assert_eq!(header(&response.headers, "content-type"), vec!["application/json"], "обычные заголовки ответа остаются");
    }

    #[tokio::test(start_paused = true)]
    async fn big_bodies_are_refused_before_the_upstream_hears_about_them() {
        let up = fake(None, 1);
        assert_eq!(proxy(request("/auth/register", &[], 1024), &up, &config()).await.status, 200, "ровно по лимиту проходит");
        assert_eq!(proxy(request("/auth/register", &[], 1025), &up, &config()).await.status, 413, "на байт больше это 413");
        assert_eq!(up.seen.lock().unwrap().len(), 1, "слишком большое тело до апстрима не дошло");
    }

    #[tokio::test(start_paused = true)]
    async fn upstream_trouble_is_502_or_504_without_details() {
        let up = fake(Some(Err(UpstreamError::Connect)), 1);
        let response = proxy(request("/auth/login", &[], 0), &up, &config()).await;
        assert_eq!(response.status, 502, "апстрим недоступен");
        let slow = fake(None, 600);
        let started = tokio::time::Instant::now();
        let response = proxy(request("/auth/login", &[], 0), &slow, &config()).await;
        assert_eq!(response.status, 504, "апстрим не ответил в срок");
        assert_eq!(started.elapsed(), Duration::from_millis(500), "ждали ровно timeout");
        let reset = fake(Some(Err(UpstreamError::Reset)), 1);
        let response = proxy(request("/auth/login", &[], 0), &reset, &config()).await;
        let text = String::from_utf8(response.body).unwrap();
        assert!(!text.contains("Reset") && !text.contains("Connect"), "тело не раскрывает устройство апстрима: {text}");
        assert_eq!(response.status, 502, "обрыв это тоже 502");
    }
}
