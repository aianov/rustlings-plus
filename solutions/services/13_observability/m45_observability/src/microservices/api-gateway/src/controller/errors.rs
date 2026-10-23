use std::collections::HashMap;
use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Outcome, decode};
use http_engine::{Middleware, Request, Response, handler, middleware};
use proto::new_request_id;

pub(crate) const REQUEST_ID: &str = "x-request-id";

pub(crate) fn request_id(request: &Request) -> String {
    request.headers.get(REQUEST_ID).unwrap_or("-").to_string()
}

fn envelope(status: u16, text: &str) -> Option<ErrorBody> {
    if let Ok(Outcome::<()>::Error(body)) = decode::<Outcome<()>>(text) {
        return Some(body);
    }
    let code = ErrorCode::for_status(status);
    let bare = decode::<HashMap<String, String>>(text).ok();
    let message = match bare.as_ref().and_then(|bare| bare.get("error")) {
        Some(message) if code.is_client_fault() => message.clone(),
        _ => code.public_message().to_string(),
    };
    Some(ErrorBody::new(code, message))
}

fn stamped(mut response: Response, rid: &str) -> Response {
    response.headers.set(REQUEST_ID, rid);
    let status = response.status.code();
    if status < 400 || response.body.is_empty() {
        return response;
    }
    let text = String::from_utf8_lossy(&response.body).to_string();
    let Some(body) = envelope(status, &text) else {
        return response;
    };
    if status >= 500 && body.code != ErrorCode::Unavailable.as_str() {
        tracing::warn!("api-gateway [{rid}]: ответ {status}: {text}");
    }
    let mut stamped = Response::json(status, &Outcome::<()>::Error(body.with_request_id(rid)));
    for (name, value) in response.headers.iter() {
        if !name.eq_ignore_ascii_case("content-type")
            && !name.eq_ignore_ascii_case("content-length")
        {
            stamped.headers.append(name, value);
        }
    }
    stamped
}

pub(crate) fn request_ids() -> Middleware {
    middleware(|next| {
        handler(move |mut request: Request| {
            let next = Arc::clone(&next);
            let rid = new_request_id();
            request.headers.set(REQUEST_ID, &rid);
            async move { stamped(next(request).await, &rid) }
        })
    })
}
