use crate::{Method, Request, Response};

pub fn etag(body: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in body {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("\"{hash:016x}\"")
}

pub fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let _ = (if_none_match, etag);
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSpec {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

pub fn parse_range(header: Option<&str>, len: u64) -> RangeSpec {
    let _ = (header, len);
    todo!()
}

pub fn serve_bytes(request: &Request, content_type: &str, data: &[u8]) -> Response {
    let _ = (request, content_type, data);
    todo!()
}
