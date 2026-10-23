use std::sync::Arc;

use contracts::{DomainEvent, Topics, UserId};
use nats_engine::{Bus, BusError, Pattern, Subscription};
use service_runtime::CancellationToken;

use crate::common::Guards;

pub const PROFILE_CHANGED: [&str; 2] = ["user", "profile_changed"];

const ANY_AGGREGATE: char = '*';

pub async fn subscribe(bus: &Bus, topics: &Topics) -> Result<Subscription<DomainEvent>, BusError> {
    let mut words = topics.all_events();
    words.pop();
    words.extend(PROFILE_CHANGED.iter().map(|word| (*word).to_string()));
    words.push(ANY_AGGREGATE.to_string());
    let pattern =
        Pattern::parse(&words.join(".")).map_err(|error| BusError::Transport(error.to_string()))?;
    bus.subscribe(&pattern).await
}

pub async fn forget_changed(
    mut events: Subscription<DomainEvent>,
    guards: Arc<Guards>,
    closing: CancellationToken,
) {
    loop {
        let next = tokio::select! {
            _ = closing.cancelled() => break,
            next = events.next() => next,
        };
        match next {
            Some(Ok(envelope)) => {
                let event = envelope.body;
                if let Ok(user) = event.aggregate_id.parse::<UserId>() {
                    guards.profiles.invalidate(user, event.version);
                    if let Some(cached) = &guards.cached {
                        cached.invalidate(user).await;
                    }
                }
            }
            Some(Err(error)) => tracing::warn!("событие профиля не разобрано: {error}"),
            None => break,
        }
    }
    events.stop().await;
}
