use crate::{Bus, Ids};
use obs::TRACEPARENT;
use obs::metrics::Registry;
use obs::red::Red;
use obs::trace::enter;
use std::sync::Arc;
use tracing::{Instrument, info};

pub struct NotifyService {
    registry: Arc<Registry>,
    red: Red,
    bus: Bus,
    ids: Ids,
}

impl NotifyService {
    pub fn new(bus: Bus, ids: Ids) -> NotifyService {
        let registry = Arc::new(Registry::new());
        NotifyService { red: Red::new(registry.clone()), registry, bus, ids }
    }

    pub fn metrics(&self) -> String {
        self.registry.render()
    }

    pub async fn run_once(&self) -> usize {
        let mut handled = 0;
        while let Some(message) = self.bus.next() {
            let entered = enter(message.headers.get(TRACEPARENT).map(String::as_str), &mut || (self.ids)());
            let span = obs::hop_span("notify", &entered.ctx, entered.parent);
            let subject = message.subject.clone();
            let work = async {
                info!(subject = %message.subject, "push отправлен подписчикам");
                (200, ())
            };
            self.red.track(&subject, work).instrument(span).await;
            handled += 1;
        }
        handled
    }
}
