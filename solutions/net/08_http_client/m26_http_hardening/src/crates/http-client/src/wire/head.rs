use crate::url::Headers;

use super::is_token;

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub minor: u8,
    pub status: u16,
    pub reason: String,
    pub headers: Headers,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

pub const MAX_HEAD: usize = 16 * 1024;

impl Head {
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    pub fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.all("connection") {
            for token in value.split(',') {
                close |= token.trim().eq_ignore_ascii_case("close");
                keep |= token.trim().eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.minor == 1)
    }
}

pub fn parse_head(buf: &[u8]) -> Result<Option<Head>, HeadError> {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() > MAX_HEAD {
            Err(HeadError::TooLarge)
        } else {
            Ok(None)
        };
    };
    let len = end + 4;
    if len > MAX_HEAD {
        return Err(HeadError::TooLarge);
    }
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| HeadError::Header)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let (version, rest) = first.split_once(' ').ok_or(HeadError::StatusLine)?;
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        v if v.starts_with("HTTP/") => return Err(HeadError::Version),
        _ => return Err(HeadError::StatusLine),
    };
    let (code, reason) = rest.split_once(' ').unwrap_or((rest, ""));
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HeadError::Status);
    }
    let status: u16 = code.parse().map_err(|_| HeadError::Status)?;
    if status < 100 {
        return Err(HeadError::Status);
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.starts_with([' ', '\t']) {
            return Err(HeadError::Header);
        }
        let (name, value) = line.split_once(':').ok_or(HeadError::Header)?;
        if !is_token(name) || value.contains(['\r', '\n']) {
            return Err(HeadError::Header);
        }
        headers.push((
            name.to_string(),
            value.trim_matches([' ', '\t']).to_string(),
        ));
    }
    Ok(Some(Head {
        minor,
        status,
        reason: reason.to_string(),
        headers,
        len,
    }))
}
