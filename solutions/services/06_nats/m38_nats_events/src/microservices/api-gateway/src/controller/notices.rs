use std::sync::Arc;

use contracts::{NoticeView, Topics};
use nats_engine::{Bus, BusError, Envelope, Pattern, Subscription};
use service_runtime::CancellationToken;

use crate::common::Registry;

pub async fn subscribe(bus: &Bus, topics: &Topics) -> Result<Subscription<NoticeView>, BusError> {
    let pattern = Pattern::parse(&topics.all_notices().join("."))
        .map_err(|error| BusError::Transport(error.to_string()))?;
    bus.subscribe(&pattern).await
}

pub async fn forward(
    mut notices: Subscription<NoticeView>,
    topics: Topics,
    registry: Arc<Registry>,
    closing: CancellationToken,
) {
    loop {
        let item = tokio::select! {
            () = closing.cancelled() => break,
            item = notices.next() => item,
        };
        match item {
            Some(Ok(envelope)) => deliver(&topics, &registry, envelope),
            Some(Err(error)) => eprintln!("api-gateway: уведомление не разобрано: {error}"),
            None => return,
        }
    }
    notices.stop().await;
}

fn deliver(topics: &Topics, registry: &Registry, envelope: Envelope<NoticeView>) {
    let notice = envelope.body;
    if topics.read_notice(envelope.subject.as_str()) != Some(notice.to) {
        eprintln!(
            "api-gateway: уведомление {} для {} пришло в чужую тему {}",
            notice.id, notice.to, envelope.subject
        );
        return;
    }
    if let Some(push) = notice.push() {
        registry.deliver(notice.to, &push);
    }
}
