#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub created_ms: u64,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BadCursor;

fn checksum(body: &str) -> u16 {
    let mut h: u32 = 0x811c9dc5;
    for b in body.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(16777619);
    }
    (h ^ (h >> 16)) as u16
}

pub fn encode_cursor(created_ms: u64, id: u64) -> String {
    let body = format!("{created_ms:016x}{id:016x}");
    format!("{body}{:04x}", checksum(&body))
}

pub fn decode_cursor(s: &str) -> Result<(u64, u64), BadCursor> {
    if s.len() != 36 || !s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(BadCursor);
    }
    let (body, sum) = s.split_at(32);
    if u16::from_str_radix(sum, 16).map_err(|_| BadCursor)? != checksum(body) {
        return Err(BadCursor);
    }
    let created = u64::from_str_radix(&body[..16], 16).map_err(|_| BadCursor)?;
    let id = u64::from_str_radix(&body[16..], 16).map_err(|_| BadCursor)?;
    Ok((created, id))
}
