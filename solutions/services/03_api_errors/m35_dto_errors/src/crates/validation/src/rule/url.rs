use super::{Rule, Violation};

pub const MAX_URL: usize = 2048;

pub struct HttpUrl;

fn authority_ok(authority: &str) -> bool {
    if authority.contains('@') {
        return false;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let host_ok = !host.is_empty()
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && !host.contains("..")
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let port_ok = port.is_none_or(|port| port.parse::<u16>().is_ok_and(|n| n > 0));
    host_ok && port_ok
}

impl Rule<str> for HttpUrl {
    fn check(&self, value: &str) -> Result<(), Violation> {
        if value.chars().count() > MAX_URL {
            let message = format!("ссылка длиннее {MAX_URL} символов");
            return Err(Violation::new("too_long", message));
        }
        let lower = value.to_ascii_lowercase();
        let rest = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"));
        let well_formed = rest.is_some_and(|rest| {
            let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            authority_ok(&rest[..end])
                && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        });
        if well_formed {
            Ok(())
        } else {
            let message = "нужна ссылка http:// или https:// с именем хоста";
            Err(Violation::new("invalid_url", message))
        }
    }
}
