use std::net::{AddrParseError, IpAddr, SocketAddr};

fn parse_addr(text: &str) -> Result<SocketAddr, AddrParseError> {
    text.parse()
}

fn from_parts(ip: IpAddr, port: u16) -> SocketAddr {
    SocketAddr::new(ip, port)
}

fn with_port(mut addr: SocketAddr, port: u16) -> SocketAddr {
    addr.set_port(port);
    addr
}

fn is_ipv6(addr: &SocketAddr) -> bool {
    addr.is_ipv6()
}

fn main() {
    let addr = parse_addr("[::1]:443").unwrap();
    println!("{addr}: ipv6 = {}", is_ipv6(&addr));
    println!("{}", with_port(from_parts(addr.ip(), 1), 2));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn parses_ipv4() {
        let addr = parse_addr("127.0.0.1:8080").expect("обычный IPv4 с портом должен разбираться");
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST), "не тот IP после разбора");
        assert_eq!(addr.port(), 8080, "не тот порт после разбора");
    }

    #[test]
    fn parses_ipv6_in_brackets() {
        let addr = parse_addr("[::1]:443").expect("IPv6 в квадратных скобках должен разбираться");
        assert_eq!(addr.ip(), IpAddr::V6(Ipv6Addr::LOCALHOST), "не тот IPv6 после разбора");
        assert_eq!(addr.port(), 443, "не тот порт после разбора");
        assert!(is_ipv6(&addr), "is_ipv6 должна узнавать адрес v6");
        assert!(!is_ipv6(&parse_addr("10.0.0.1:1").unwrap()), "10.0.0.1 это IPv4");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_addr("127.0.0.1").is_err(), "без порта это не SocketAddr");
        assert!(parse_addr("127.0.0.1:99999").is_err(), "порт больше 65535 не бывает");
        assert!(parse_addr("::1:443").is_err(), "IPv6 с портом без скобок неоднозначен и должен быть ошибкой");
        assert!(parse_addr("localhost:80").is_err(), "имена хостов разбор не резолвит, для этого есть lookup_host");
        assert!(parse_addr("").is_err(), "пустая строка не адрес");
    }

    #[test]
    fn builds_from_parts() {
        let addr = from_parts(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7)), 3000);
        assert_eq!(addr.to_string(), "192.168.1.7:3000", "сборка из IP и порта дала не тот адрес");
        let v6 = from_parts(IpAddr::V6(Ipv6Addr::LOCALHOST), 80);
        assert_eq!(v6.to_string(), "[::1]:80", "в тексте IPv6 с портом должен быть в квадратных скобках");
    }

    #[test]
    fn replaces_port_only() {
        let addr = parse_addr("10.1.2.3:80").unwrap();
        let moved = with_port(addr, 9000);
        assert_eq!(moved.ip(), addr.ip(), "with_port не должна менять IP");
        assert_eq!(moved.port(), 9000, "with_port должна менять порт");
        assert_eq!(addr.port(), 80, "исходный адрес Copy и менять его не нужно");
    }
}
