mod events;
mod requests;

use std::sync::Arc;

use nats_engine::{Bus, BusError, Pattern, Server};

use crate::common::{Config, Stats};
use crate::service::NotifyService;

use events::Intake;

pub struct Controller {
    intake: Intake,
    servers: Vec<Server>,
}

pub(crate) fn pattern(words: &[String]) -> Result<Pattern, BusError> {
    Pattern::parse(&words.join(".")).map_err(|error| BusError::Transport(error.to_string()))
}

pub async fn start(
    bus: &Bus,
    config: &Config,
    service: Arc<NotifyService>,
    stats: Arc<Stats>,
) -> Result<Controller, BusError> {
    let intake = events::start(bus, config, Arc::clone(&service), stats).await?;
    let servers = requests::start(bus, config, service).await?;
    Ok(Controller { intake, servers })
}

impl Controller {
    pub async fn stop(self) {
        let workers = self.intake.close().await;
        for server in self.servers {
            server.stop().await;
        }
        workers.finish().await;
    }
}
