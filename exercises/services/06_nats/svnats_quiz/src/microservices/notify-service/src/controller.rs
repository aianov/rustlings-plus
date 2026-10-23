use crate::common::{Config, Stats};
use crate::service::NotifyService;
use nats_engine::{Bus, BusError};
use std::sync::Arc;

pub struct Controller {
    // TODO: поля на твоё усмотрение: что нужно, чтобы остановить приём событий,
    // дождаться работников и снять ответчиков
}

pub async fn start(bus: Arc<Bus>, config: &Config, service: Arc<NotifyService>, stats: Arc<Stats>) -> Result<Controller, BusError> {
    todo!()
}

impl Controller {
    pub async fn stop(self) {
        todo!()
    }
}
