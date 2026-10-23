use async_nats::HeaderMap;
use async_nats::jetstream::{Context, Message};

use crate::error::{BusError, transport};
use crate::meta::Meta;
use crate::subject::Subject;

pub const ORIGINAL_SUBJECT: &str = "X-Original-Subject";

pub const DELIVERY_COUNT: &str = "X-Delivery-Count";

pub const ERROR: &str = "X-Error";

pub(crate) async fn dead_letter(
    js: &Context,
    message: &Message,
    dead: &Subject,
    reason: &str,
) -> Result<(), BusError> {
    let delivered = message.info().map(|info| info.delivered).unwrap_or(0);
    let mut headers = match Meta::from_headers(message.headers.as_ref()) {
        Some(meta) => meta.to_headers(),
        None => HeaderMap::new(),
    };
    headers.insert(ORIGINAL_SUBJECT, message.subject.as_str());
    headers.insert(DELIVERY_COUNT, delivered.to_string().as_str());
    headers.insert(ERROR, reason.replace(['\r', '\n'], " ").as_str());
    let ack = js
        .publish_with_headers(dead.as_str().to_string(), headers, message.payload.clone())
        .await
        .map_err(transport)?;
    ack.await.map_err(transport)?;
    Ok(())
}
