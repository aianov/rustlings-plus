pub mod auth;
pub mod gateway;
pub mod notify;
pub mod posts;
mod rpc;
pub mod users;

pub use rpc::{Service, request_json, sync};

use async_nats::Client;
use std::net::SocketAddr;

pub struct Stack {
    pub addr: SocketAddr,
    pub prefix: String,
    services: Vec<Service>,
}

impl Stack {
    pub async fn start(client: &Client, prefix: &str) -> Stack {
        let _ = (client, prefix, auth::start, users::start, posts::start, notify::start, gateway::start);
        todo!()
    }

    pub async fn stop(self) {
        let _ = self.services;
        todo!()
    }

    pub fn live_tasks(&self) -> usize {
        self.services.iter().map(Service::live).sum()
    }
}
