use crate::method::Method;
use crate::request::Request;
use crate::response::Response;

pub fn etag(body: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in body {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("\"{hash:016x}\"")
}

pub fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let mut candidates = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in if_none_match.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            ',' if !quoted => candidates.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    candidates.push(current);
    candidates.iter().map(|c| c.trim()).any(|c| {
        let c = c.strip_prefix("W/").unwrap_or(c);
        c == "*" || (c.len() >= 2 && c.starts_with('"') && c.ends_with('"') && c == etag)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSpec {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

pub fn parse_range(header: Option<&str>, len: u64) -> RangeSpec {
    let Some(header) = header else {
        return RangeSpec::Full;
    };
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return RangeSpec::Full;
    };
    if spec.contains(',') {
        return RangeSpec::Full;
    }
    let Some((first, last)) = spec.trim().split_once('-') else {
        return RangeSpec::Full;
    };
    let number = |s: &str| -> Option<u64> {
        if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse().ok()
        } else {
            None
        }
    };
    match (first.is_empty(), last.is_empty()) {
        (true, true) => RangeSpec::Full,
        (true, false) => match number(last) {
            None => RangeSpec::Full,
            Some(0) => RangeSpec::Unsatisfiable,
            Some(_) if len == 0 => RangeSpec::Unsatisfiable,
            Some(n) => RangeSpec::Partial {
                start: len.saturating_sub(n),
                end: len - 1,
            },
        },
        (false, true) => match number(first) {
            None => RangeSpec::Full,
            Some(start) if start < len => RangeSpec::Partial {
                start,
                end: len - 1,
            },
            Some(_) => RangeSpec::Unsatisfiable,
        },
        (false, false) => match (number(first), number(last)) {
            (Some(start), Some(end)) if end < start => RangeSpec::Full,
            (Some(start), Some(_)) if start >= len => RangeSpec::Unsatisfiable,
            (Some(start), Some(end)) => RangeSpec::Partial {
                start,
                end: end.min(len - 1),
            },
            _ => RangeSpec::Full,
        },
    }
}

pub fn serve_bytes(request: &Request, content_type: &str, data: &[u8]) -> Response {
    let len = data.len() as u64;
    let tag = etag(data);
    let safe = matches!(request.method, Method::Get | Method::Head);
    if safe
        && request
            .headers
            .get("if-none-match")
            .is_some_and(|v| etag_matches(v, &tag))
    {
        let mut response = Response::new(304);
        response.headers.append("ETag", &tag);
        response.headers.append("Cache-Control", "no-cache");
        return response;
    }
    let mut response = match parse_range(request.headers.get("range").filter(|_| safe), len) {
        RangeSpec::Full => {
            let mut r = Response::new(200);
            r.body = data.to_vec();
            r
        }
        RangeSpec::Partial { start, end } => {
            let mut r = Response::new(206);
            r.headers
                .append("Content-Range", &format!("bytes {start}-{end}/{len}"));
            let (from, to) = (
                usize::try_from(start).unwrap_or(0),
                usize::try_from(end).unwrap_or(0),
            );
            r.body = data[from..=to].to_vec();
            r
        }
        RangeSpec::Unsatisfiable => {
            let mut r = Response::new(416);
            r.headers.append("Content-Range", &format!("bytes */{len}"));
            r.headers.append("Accept-Ranges", "bytes");
            return r;
        }
    };
    response.headers.append("Content-Type", content_type);
    response.headers.append("Accept-Ranges", "bytes");
    response.headers.append("ETag", &tag);
    response.headers.append("Cache-Control", "no-cache");
    response
}
