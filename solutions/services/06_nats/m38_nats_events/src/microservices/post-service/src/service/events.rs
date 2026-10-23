use contracts::{Event, Origin};

use crate::repository::BoxFuture;

pub trait Events: Send + Sync {
    fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()>;
}

pub struct Silent;

impl Events for Silent {
    fn publish(&self, _: Event, _: Origin) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}
