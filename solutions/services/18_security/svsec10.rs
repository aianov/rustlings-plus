
fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    MissingOrigin,
    ForbiddenOrigin,
}

pub struct Handshake<'a> {
    pub origin: Option<&'a str>,
    pub cookie_auth: bool,
}

/// Приводит Origin к виду `схема://имя[:порт]` без стандартного порта, нижний регистр. Всё лишнее (путь, user@, пробелы) это `None`.
fn normalize(origin: &str) -> Option<String> {
    let (scheme, rest) = origin.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    if rest.is_empty() || rest.contains(['/', '@', '?', '#', ' ', '\\']) {
        return None;
    }
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') || host.contains(']') => (host, port.parse::<u16>().ok()?),
        _ => (rest, default_port),
    };
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    Some(if port == default_port { format!("{scheme}://{host}") } else { format!("{scheme}://{host}:{port}") })
}

pub fn check_origin(h: &Handshake, allowed: &[&str]) -> Result<(), Reject> {
    let Some(origin) = h.origin else {
        return if h.cookie_auth { Err(Reject::MissingOrigin) } else { Ok(()) };
    };
    let origin = normalize(origin).ok_or(Reject::ForbiddenOrigin)?;
    if allowed.iter().filter_map(|a| normalize(a)).any(|a| a == origin) { Ok(()) } else { Err(Reject::ForbiddenOrigin) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: [&str; 2] = ["https://app.example.com", "http://localhost:3000"];

    fn cookie(origin: Option<&str>) -> Result<(), Reject> {
        check_origin(&Handshake { origin, cookie_auth: true }, &ALLOWED)
    }

    #[test]
    fn the_allowed_origins_pass() {
        for ok in ["https://app.example.com", "https://APP.example.COM", "HTTPS://app.example.com", "https://app.example.com:443", "http://localhost:3000"] {
            assert_eq!(cookie(Some(ok)), Ok(()), "{ok:?} из белого списка отвергнут");
        }
    }

    #[test]
    fn lookalikes_are_rejected() {
        for bad in [
            "https://app.example.com.evil.com",
            "https://evilapp.example.com",
            "https://app.example.com@evil.com",
            "https://evil.com/https://app.example.com",
            "http://app.example.com",
            "https://app.example.com:8443",
            "http://localhost",
            "http://localhost:3001",
            "http://localhost:30000",
            "https://app.example.com/",
            "null",
            "",
            "app.example.com",
            "ftp://app.example.com",
            "https://",
            "https://app.example.com\\.evil.com",
        ] {
            assert_eq!(cookie(Some(bad)), Err(Reject::ForbiddenOrigin), "{bad:?} прошёл, а это чужой сайт или подделка");
        }
    }

    #[test]
    fn cookie_sessions_require_an_origin() {
        assert_eq!(cookie(None), Err(Reject::MissingOrigin), "браузер всегда присылает Origin; его отсутствие при куки подозрительно");
    }

    #[test]
    fn token_clients_may_omit_it_but_not_lie() {
        let token = |origin| check_origin(&Handshake { origin, cookie_auth: false }, &ALLOWED);
        assert_eq!(token(None), Ok(()), "мобильное приложение и скрипты не шлют Origin");
        assert_eq!(token(Some("https://evil.com")), Err(Reject::ForbiddenOrigin), "если Origin есть, он проверяется всегда");
        assert_eq!(token(Some("https://app.example.com")), Ok(()));
    }
}
