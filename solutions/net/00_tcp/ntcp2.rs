use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

fn connectable_addr(bind: SocketAddr) -> SocketAddr {
    match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => SocketAddr::new(Ipv4Addr::LOCALHOST.into(), bind.port()),
        IpAddr::V6(ip) if ip.is_unspecified() => SocketAddr::new(Ipv6Addr::LOCALHOST.into(), bind.port()),
        _ => bind,
    }
}

fn is_local_only(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

fn main() {
    let bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("слушаем {bind}, подключаемся к {}", connectable_addr(bind));
    println!("только эта машина: {}", is_local_only(&bind));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn wildcard_v4_becomes_loopback() {
        assert_eq!(
            connectable_addr(addr("0.0.0.0:8080")),
            addr("127.0.0.1:8080"),
            "для сервера на 0.0.0.0 клиент должен идти на 127.0.0.1 с тем же портом"
        );
    }

    #[test]
    fn wildcard_v6_becomes_loopback() {
        assert_eq!(
            connectable_addr(addr("[::]:9000")),
            addr("[::1]:9000"),
            "для сервера на [::] клиент должен идти на [::1] с тем же портом"
        );
    }

    #[test]
    fn concrete_addresses_stay() {
        for s in ["127.0.0.1:1", "192.168.1.5:80", "[::1]:5", "10.0.0.2:443"] {
            assert_eq!(connectable_addr(addr(s)), addr(s), "конкретный адрес {s} менять не нужно");
        }
    }

    #[test]
    fn local_only_means_loopback() {
        assert!(is_local_only(&addr("127.0.0.1:80")), "127.0.0.1 виден только этой машине");
        assert!(is_local_only(&addr("[::1]:80")), "[::1] виден только этой машине");
        assert!(!is_local_only(&addr("0.0.0.0:80")), "0.0.0.0 это все интерфейсы, наружу тоже");
        assert!(!is_local_only(&addr("[::]:80")), "[::] это все интерфейсы, наружу тоже");
        assert!(!is_local_only(&addr("192.168.1.5:80")), "адрес из локальной сети виден соседям");
    }
}
