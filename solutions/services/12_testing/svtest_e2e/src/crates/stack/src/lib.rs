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
        let auth = auth::start(client, prefix).await;
        let users = users::start(client, prefix).await;
        let posts = posts::start(client, prefix).await;
        let notify = notify::start(client, prefix).await;
        let gateway = gateway::start(client, prefix).await;
        Stack { addr: gateway.addr, prefix: prefix.to_string(), services: vec![gateway.service, notify, posts, users, auth] }
    }

    pub async fn stop(self) {
        for service in self.services {
            service.stop().await;
        }
    }

    pub fn live_tasks(&self) -> usize {
        self.services.iter().map(Service::live).sum()
    }
}
