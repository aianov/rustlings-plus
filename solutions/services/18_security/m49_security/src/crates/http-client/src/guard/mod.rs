use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::error::Error;
use crate::url::Url;

type Check = Arc<dyn Fn(SocketAddr) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct AddrGuard {
    check: Check,
}

impl AddrGuard {
    pub fn new(check: impl Fn(SocketAddr) -> bool + Send + Sync + 'static) -> AddrGuard {
        AddrGuard {
            check: Arc::new(check),
        }
    }

    pub fn public_only() -> AddrGuard {
        AddrGuard::new(|addr| public_ip(addr.ip()))
    }

    pub fn allows(&self, addr: SocketAddr) -> bool {
        (self.check)(addr)
    }
}

impl fmt::Debug for AddrGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AddrGuard")
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(inner) = ip.to_ipv4_mapped() {
        return public_v4(inner);
    }
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || first == 0x2001 && ip.segments()[1] == 0x0db8)
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

pub(crate) async fn pin(
    guard: &AddrGuard,
    url: &Url,
    limit: Duration,
) -> Result<SocketAddr, Error> {
    let found = match timeout(
        limit,
        tokio::net::lookup_host((url.host.as_str(), url.port)),
    )
    .await
    {
        Err(_) => return Err(Error::ConnectTimeout),
        Ok(Err(e)) => return Err(Error::Connect(e)),
        Ok(Ok(found)) => found,
    };
    let addrs: Vec<SocketAddr> = found.collect();
    match addrs.first() {
        Some(first) if addrs.iter().all(|addr| guard.allows(*addr)) => Ok(*first),
        Some(..) | None => Err(Error::Forbidden(url.host.clone())),
    }
}
