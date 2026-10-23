use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Outcome};
use http_engine::{Headers, Request, Response};

use super::errors::request_id;
use super::http::fail;
use crate::bootstrap::Shared;
use crate::client::Relayed;

pub(crate) const PREFIX: &str = "/auth/";

const HOP_BY_HOP: [&str; 8] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const ABOUT_CLIENT: [&str; 5] = [
    "x-forwarded-for",
    "x-real-ip",
    "forwarded",
    "x-forwarded-proto",
    "x-forwarded-host",
];

const INTERNAL: [&str; 4] = ["x-user", "x-user-id", "x-user-role", "x-request-id"];

const FRAMING: [&str; 2] = ["host", "content-length"];

const TRAVERSAL: [&str; 6] = ["..", "//", "\\", "%2e", "%2f", "%5c"];

pub(crate) fn is_auth(request: &Request) -> bool {
    request.raw_path().starts_with(PREFIX)
}

fn listed(headers: &Headers) -> Vec<String> {
    headers
        .get_all("connection")
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

fn hop(name: &str, listed: &[String]) -> bool {
    HOP_BY_HOP.contains(&name) || listed.iter().any(|own| own == name)
}

fn outward(headers: &Headers) -> Vec<(String, String)> {
    let listed = listed(headers);
    headers
        .iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            !hop(&name, &listed)
                && !ABOUT_CLIENT.contains(&name.as_str())
                && !INTERNAL.contains(&name.as_str())
                && !FRAMING.contains(&name.as_str())
                && !name.starts_with("x-internal-")
        })
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn inward(headers: Vec<(String, String)>) -> Headers {
    let mut kept = Headers::new();
    for (name, value) in &headers {
        kept.append(name, value);
    }
    let listed = listed(&kept);
    let mut back = Headers::new();
    for (name, value) in &headers {
        let lower = name.to_ascii_lowercase();
        if !hop(&lower, &listed) && !FRAMING.contains(&lower.as_str()) && lower != "x-request-id" {
            back.append(name, value);
        }
    }
    back
}

fn clean(request: &Request) -> bool {
    let path = request.raw_path().to_ascii_lowercase();
    path.len() > PREFIX.len() && !TRAVERSAL.iter().any(|bad| path.contains(bad))
}

fn gateway_error(status: u16, code: ErrorCode, message: &str) -> Response {
    Response::json(status, &Outcome::<()>::Error(ErrorBody::new(code, message)))
}

pub(crate) async fn auth(shared: Arc<Shared>, request: Request) -> Response {
    if !clean(&request) {
        return fail(ErrorCode::NotFound, "у входа нет такого пути");
    }
    let limit = shared.config.proxy.max_body;
    if request.body.len() > limit {
        return fail(
            ErrorCode::TooLarge,
            &format!("тело запроса ко входу больше {limit} байт"),
        );
    }
    let rid = request_id(&request);
    let mut headers = outward(&request.headers);
    headers.push((String::from("X-Request-Id"), rid.clone()));
    if let Some(peer) = request.peer {
        headers.push((String::from("X-Forwarded-For"), peer.ip().to_string()));
        headers.push((String::from("X-Real-IP"), peer.ip().to_string()));
    }
    let proto = if shared.config.tls.is_some() {
        "https"
    } else {
        "http"
    };
    headers.push((String::from("X-Forwarded-Proto"), proto.to_string()));
    let method = request.method.as_str();
    let sent = shared
        .auth_http
        .send(method, &request.target, headers, request.body, &rid)
        .await;
    match sent {
        Relayed::Answer(answer) => {
            let mut response = Response::new(answer.status);
            response.headers = inward(answer.headers);
            response.body = answer.body;
            response
        }
        Relayed::Unreachable => gateway_error(
            502,
            ErrorCode::Unavailable,
            "вход сейчас недоступен, повтори позже",
        ),
        Relayed::Late => gateway_error(
            504,
            ErrorCode::Timeout,
            "вход не ответил вовремя, повтори позже",
        ),
    }
}
