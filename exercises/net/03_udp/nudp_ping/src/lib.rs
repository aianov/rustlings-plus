use std::io;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub sent: u32,
    pub received: u32,
    pub lost: u32,
    pub duplicates: u32,
    pub reordered: u32,
    pub rtt_min: Option<Duration>,
    pub rtt_max: Option<Duration>,
    pub rtt_avg: Option<Duration>,
}

impl Report {
    pub fn loss_percent(&self) -> f64 {
        todo!()
    }
}

pub async fn ping(server: SocketAddr, count: u32, interval: Duration, wait: Duration) -> io::Result<Report> {
    todo!()
}
