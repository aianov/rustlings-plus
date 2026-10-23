use std::io;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::Instant;

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
        if self.sent == 0 {
            0.0
        } else {
            f64::from(self.lost) * 100.0 / f64::from(self.sent)
        }
    }
}

pub async fn ping(server: SocketAddr, count: u32, interval: Duration, wait: Duration) -> io::Result<Report> {
    let bind = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let socket = UdpSocket::bind(bind).await?;
    socket.connect(server).await?;
    let mut report = Report::default();
    if count == 0 {
        return Ok(report);
    }
    let mut sent_at: Vec<Instant> = Vec::with_capacity(count as usize);
    let mut seen = vec![false; count as usize];
    let mut highest: Option<u32> = None;
    let mut rtts: Vec<Duration> = Vec::new();
    let mut ticker = tokio::time::interval(interval);
    let mut deadline: Option<Instant> = None;
    let mut buf = [0u8; 64];
    loop {
        let stop = deadline.unwrap_or_else(Instant::now);
        tokio::select! {
            _ = ticker.tick(), if deadline.is_none() => {
                let seq = sent_at.len() as u32;
                let mut packet = seq.to_be_bytes().to_vec();
                packet.extend_from_slice(b"ping");
                sent_at.push(Instant::now());
                socket.send(&packet).await?;
                if sent_at.len() as u32 == count {
                    deadline = Some(Instant::now() + wait);
                }
            }
            got = socket.recv(&mut buf) => {
                let n = got?;
                if n < 4 {
                    continue;
                }
                let seq = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if seq as usize >= sent_at.len() {
                    continue;
                }
                if seen[seq as usize] {
                    report.duplicates += 1;
                    continue;
                }
                seen[seq as usize] = true;
                if highest.is_some_and(|h| seq < h) {
                    report.reordered += 1;
                }
                highest = Some(highest.map_or(seq, |h| h.max(seq)));
                rtts.push(sent_at[seq as usize].elapsed());
                report.received += 1;
            }
            _ = tokio::time::sleep_until(stop), if deadline.is_some() => break,
        }
    }
    report.sent = count;
    report.lost = count - report.received;
    report.rtt_min = rtts.iter().min().copied();
    report.rtt_max = rtts.iter().max().copied();
    if !rtts.is_empty() {
        report.rtt_avg = Some(rtts.iter().sum::<Duration>() / rtts.len() as u32);
    }
    Ok(report)
}
