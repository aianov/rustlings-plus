use async_nats::{Client, HeaderMap, Message};
use rand::Rng;

fn main() {}

pub const REQUEST_ID: &str = "X-Request-Id";
pub const TRACE_ID: &str = "X-Trace-Id";

#[derive(Debug, Clone, PartialEq)]
pub struct TraceCtx {
    pub request_id: String,
    pub trace_id: String,
}

fn good_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn fresh(prefix: &str) -> String {
    format!("{prefix}-{:016x}", rand::thread_rng().r#gen::<u64>())
}

pub async fn publish_with_ctx(client: &Client, subject: String, payload: Vec<u8>, ctx: &TraceCtx) -> Result<(), String> {
    if !good_id(&ctx.request_id) || !good_id(&ctx.trace_id) {
        return Err("негодный идентификатор в контексте".to_string());
    }
    let mut headers = HeaderMap::new();
    headers.insert(REQUEST_ID, ctx.request_id.as_str());
    headers.insert(TRACE_ID, ctx.trace_id.as_str());
    client.publish_with_headers(subject, headers, payload.into()).await.map_err(|e| e.to_string())
}

pub fn ctx_of(message: &Message) -> Option<TraceCtx> {
    let headers = message.headers.as_ref()?;
    let request_id = headers.get(REQUEST_ID)?.as_str();
    let trace_id = headers.get(TRACE_ID)?.as_str();
    (good_id(request_id) && good_id(trace_id)).then(|| TraceCtx { request_id: request_id.to_string(), trace_id: trace_id.to_string() })
}

pub async fn forward(client: &Client, incoming: &Message, to: String) -> Result<TraceCtx, String> {
    let ctx = ctx_of(incoming).unwrap_or_else(|| TraceCtx { request_id: fresh("req"), trace_id: fresh("trace") });
    publish_with_ctx(client, to, incoming.payload.to_vec(), &ctx).await?;
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, listen, recv};
    use std::time::Duration;

    fn ctx(request: &str, trace: &str) -> TraceCtx {
        TraceCtx { request_id: request.to_string(), trace_id: trace.to_string() }
    }

    async fn raw_with_headers(client: &Client, subject: String, pairs: &[(&str, &str)]) {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, *value);
        }
        client.publish_with_headers(subject, headers, "тело".into()).await.unwrap();
    }

    #[tokio::test]
    async fn identifiers_travel_in_headers_and_the_body_stays_untouched() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let client = connect().await;
        let mut sub = listen(&client, subject.clone()).await;
        publish_with_ctx(&client, subject, b"{\"post\":1}".to_vec(), &ctx("req-1", "trace-9")).await.expect("публикация");
        let message = recv(&mut sub, Duration::from_secs(2)).await;
        assert_eq!(message.payload.as_ref(), b"{\"post\":1}", "тело должно дойти как есть");
        let headers = message.headers.as_ref().expect("заголовки должны ехать вместе с сообщением");
        assert_eq!(headers.get(REQUEST_ID).map(|v| v.as_str()), Some("req-1"), "X-Request-Id в заголовке");
        assert_eq!(headers.get(TRACE_ID).map(|v| v.as_str()), Some("trace-9"), "X-Trace-Id в заголовке");
        assert_eq!(ctx_of(&message), Some(ctx("req-1", "trace-9")), "ctx_of читает то, что положил publish_with_ctx");
    }

    #[tokio::test]
    async fn a_hostile_identifier_cannot_forge_another_header() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let client = connect().await;
        let mut sub = listen(&client, subject.clone()).await;
        let evil = "r1\r\nX-Admin: true";
        let sent = publish_with_ctx(&client, subject.clone(), b"x".to_vec(), &ctx(evil, "t1")).await;
        assert!(sent.is_err(), "идентификатор с переводом строки отвергается, а не уходит в сеть");
        for bad in ["", "a b", "x:y", &"z".repeat(65)] {
            assert!(publish_with_ctx(&client, subject.clone(), b"x".to_vec(), &ctx(bad, "t1")).await.is_err(), "request-id {bad:?} негоден");
            assert!(publish_with_ctx(&client, subject.clone(), b"x".to_vec(), &ctx("r1", bad)).await.is_err(), "trace-id {bad:?} негоден");
        }
        client.flush().await.unwrap();
        assert!(course_support::nats::try_recv(&mut sub, Duration::from_millis(300)).await.is_none(), "по негодному контексту ничего не должно быть опубликовано");
    }

    #[tokio::test]
    async fn ctx_of_does_not_trust_what_it_reads() {
        let scope = Scope::new();
        let subject = scope.subject("in");
        let client = connect().await;
        let mut sub = listen(&client, subject.clone()).await;
        raw_with_headers(&client, subject.clone(), &[(REQUEST_ID, "ok-1")]).await;
        raw_with_headers(&client, subject.clone(), &[(REQUEST_ID, "ok-1"), (TRACE_ID, "with space")]).await;
        raw_with_headers(&client, subject.clone(), &[(REQUEST_ID, "ok-1"), (TRACE_ID, "ok-2")]).await;
        client.publish(subject, "без заголовков".into()).await.unwrap();
        let expected = [None, None, Some(ctx("ok-1", "ok-2")), None];
        for want in expected {
            let message = recv(&mut sub, Duration::from_secs(2)).await;
            assert_eq!(ctx_of(&message), want, "контекст сообщения с заголовками {:?}", message.headers);
        }
    }

    #[tokio::test]
    async fn forward_keeps_the_context_of_a_good_message() {
        let scope = Scope::new();
        let client = connect().await;
        let mut incoming_sub = listen(&client, scope.subject("in")).await;
        let mut outgoing_sub = listen(&client, scope.subject("out")).await;
        publish_with_ctx(&client, scope.subject("in"), b"hello".to_vec(), &ctx("req-77", "trace-77")).await.unwrap();
        let incoming = recv(&mut incoming_sub, Duration::from_secs(2)).await;
        let used = forward(&client, &incoming, scope.subject("out")).await.expect("пересылка");
        assert_eq!(used, ctx("req-77", "trace-77"), "forward возвращает контекст, с которым переслал");
        let outgoing = recv(&mut outgoing_sub, Duration::from_secs(2)).await;
        assert_eq!(outgoing.payload.as_ref(), b"hello", "тело то же");
        assert_eq!(ctx_of(&outgoing), Some(ctx("req-77", "trace-77")), "идентификаторы доехали до следующего сервиса");
    }

    #[tokio::test]
    async fn forward_makes_fresh_ids_when_they_are_missing_or_unfit() {
        let scope = Scope::new();
        let client = connect().await;
        let mut incoming_sub = listen(&client, scope.subject("in")).await;
        let mut outgoing_sub = listen(&client, scope.subject("out")).await;
        client.publish(scope.subject("in"), "голое".into()).await.unwrap();
        raw_with_headers(&client, scope.subject("in"), &[(REQUEST_ID, "with space"), (TRACE_ID, "t1")]).await;
        raw_with_headers(&client, scope.subject("in"), &[(REQUEST_ID, &"x".repeat(500)), (TRACE_ID, "t1")]).await;
        let mut seen = Vec::new();
        for _ in 0..3 {
            let incoming = recv(&mut incoming_sub, Duration::from_secs(2)).await;
            let used = forward(&client, &incoming, scope.subject("out")).await.expect("пересылка");
            assert!(good_id(&used.request_id) && good_id(&used.trace_id), "заведены годные идентификаторы: {used:?}");
            assert_ne!(used.trace_id, "t1", "сообщение с негодным request-id считается чужим целиком, trace-id тоже заводится заново");
            let outgoing = recv(&mut outgoing_sub, Duration::from_secs(2)).await;
            assert_eq!(ctx_of(&outgoing), Some(used.clone()), "в сообщении именно те идентификаторы, что вернул forward");
            seen.push(used);
        }
        assert!(seen[0].request_id != seen[1].request_id && seen[1].request_id != seen[2].request_id, "свежие идентификаторы разные для разных сообщений");
    }
}
