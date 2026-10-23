use crate::method::Method;
use crate::request::Version;

use super::ParseError;

pub(crate) fn percent_decode(raw: &str, plus_as_space: bool) -> Result<String, ParseError> {
    let bytes = raw.as_bytes();
    let hex = |byte: Option<&u8>| {
        byte.and_then(|byte| char::from(*byte).to_digit(16))
            .and_then(|digit| u8::try_from(digit).ok())
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => match (hex(bytes.get(at + 1)), hex(bytes.get(at + 2))) {
                (Some(high), Some(low)) => {
                    out.push((high << 4) | low);
                    at += 3;
                }
                _ => return Err(ParseError::BadEscape),
            },
            b'+' if plus_as_space => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ParseError::BadEscape)
}

pub(crate) fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

pub(super) fn parse_header_line(line: &str) -> Result<(String, String), ParseError> {
    let (name, value) = line.split_once(':').ok_or(ParseError::BadHeader)?;
    if name.is_empty() || !name.chars().all(is_token) {
        return Err(ParseError::BadHeader);
    }
    let value = value.trim_matches(|c| c == ' ' || c == '\t');
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ParseError::BadHeader);
    }
    Ok((name.to_string(), value.to_string()))
}

pub(super) fn parse_request_line(line: &str) -> Result<(Method, String, Version), ParseError> {
    let parts: Vec<&str> = line.split(' ').collect();
    let [method, target, version] = parts.as_slice() else {
        return Err(ParseError::BadRequestLine);
    };
    if method.is_empty() || target.is_empty() || version.is_empty() {
        return Err(ParseError::BadRequestLine);
    }
    let method = method.parse()?;
    let version = match *version {
        "HTTP/1.0" => Version::Http10,
        "HTTP/1.1" => Version::Http11,
        _ => return Err(ParseError::BadVersion),
    };
    if !target.starts_with('/') || target.chars().any(|c| c == '#' || c.is_control()) {
        return Err(ParseError::BadTarget);
    }
    Ok((method, target.to_string(), version))
}
