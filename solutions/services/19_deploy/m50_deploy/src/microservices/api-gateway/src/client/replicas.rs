use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use grpc_kit::{Readiness, Tls, Upstream};
use service_runtime::Background;
use tonic::transport::Channel;

use crate::common::Error;

pub const REPLICA: &str = "post-service";

struct Replica {
    addr: String,
    channel: Channel,
    health: OnceLock<Readiness>,
}

#[derive(Clone)]
pub struct Replicas {
    members: Arc<Vec<Replica>>,
    next: Arc<AtomicUsize>,
}

impl Replica {
    fn healthy(&self) -> bool {
        self.health
            .get()
            .is_none_or(|readiness| readiness.status(REPLICA).is_ok())
    }
}

pub fn addresses(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|addr| !addr.is_empty())
        .map(str::to_string)
        .collect()
}

impl Replicas {
    pub fn connect(list: &str, tls: Option<&Tls>) -> Result<Replicas, Error> {
        let mut members = Vec::new();
        for addr in addresses(list) {
            let channel =
                grpc_kit::channel(&addr, REPLICA, tls).map_err(|source| Error::Upstream {
                    service: REPLICA,
                    source,
                })?;
            members.push(Replica {
                addr,
                channel,
                health: OnceLock::new(),
            });
        }
        if members.is_empty() {
            let source = grpc_kit::channel("", REPLICA, tls).err();
            if let Some(source) = source {
                return Err(Error::Upstream {
                    service: REPLICA,
                    source,
                });
            }
        }
        Ok(Replicas {
            members: Arc::new(members),
            next: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn supervise(&self, service: &'static str, background: &mut Background) {
        for member in self.members.iter() {
            let upstream = Upstream {
                label: REPLICA,
                service,
                channel: member.channel.clone(),
            };
            let readiness = Readiness::supervised(vec![upstream], background);
            let _ = member.health.set(readiness);
        }
    }

    pub fn first(&self) -> Option<Channel> {
        self.members.first().map(|member| member.channel.clone())
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn healthy(&self) -> Vec<String> {
        self.members
            .iter()
            .filter(|member| member.healthy())
            .map(|member| member.addr.clone())
            .collect()
    }

    pub fn pick(&self) -> Option<Channel> {
        let count = self.members.len();
        let start = self.next.fetch_add(1, Ordering::Relaxed);
        let healthy = (0..count)
            .map(|step| &self.members[(start + step) % count])
            .find(|member| member.healthy());
        healthy
            .or_else(|| self.members.get(start % count.max(1)))
            .map(|member| member.channel.clone())
    }
}
