use crate::common::{Config, Notification, Stats, StatsSnapshot};
use crate::controller::{self, Controller};
use crate::repository::Store;
use crate::service::NotifyService;
use nats_engine::{Bus, BusConfig, BusError};
use std::sync::Arc;

pub struct NotifyApp {
    bus: Arc<Bus>,
    service: Arc<NotifyService>,
    stats: Arc<Stats>,
    controller: Controller,
}

impl NotifyApp {
    pub async fn start(config: Config, store: Store) -> Result<NotifyApp, BusError> {
        let bus = Arc::new(Bus::connect(BusConfig::new(config.url.clone())).await?);
        let service = Arc::new(NotifyService::new(store));
        let stats = Arc::new(Stats::default());
        let controller = controller::start(bus.clone(), &config, service.clone(), stats.clone()).await?;
        Ok(NotifyApp { bus, service, stats, controller })
    }

    pub fn ready(&self) -> bool {
        self.bus.ready()
    }

    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    pub fn unread(&self, user: &str) -> u32 {
        self.service.unread(user)
    }

    pub fn notifications(&self, user: &str) -> Vec<Notification> {
        self.service.list(user)
    }

    pub async fn stop(self) {
        self.controller.stop().await;
        self.bus.close().await.ok();
    }
}
