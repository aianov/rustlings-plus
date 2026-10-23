use super::ParseError;

const MAX_SIZE_LINE: usize = 64;

fn crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|window| window == b"\r\n")
}

pub(super) fn decode_chunked(
    buf: &[u8],
    max_body: usize,
) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    let mut at = 0;
    let mut body = Vec::new();
    loop {
        let rest = &buf[at..];
        let Some(end) = crlf(rest) else {
            if rest.len() > MAX_SIZE_LINE {
                return Err(ParseError::BadChunk);
            }
            return Ok(None);
        };
        let line = std::str::from_utf8(&rest[..end]).map_err(|_| ParseError::BadChunk)?;
        let digits = line.split(';').next().unwrap_or_default();
        if digits.is_empty() || digits.len() > 16 || !digits.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ParseError::BadChunk);
        }
        let size = usize::from_str_radix(digits, 16).map_err(|_| ParseError::BadChunk)?;
        at += end + 2;
        if size == 0 {
            loop {
                let Some(end) = crlf(&buf[at..]) else {
                    return Ok(None);
                };
                at += end + 2;
                if end == 0 {
                    return Ok(Some((body, at)));
                }
            }
        }
        if body.len().saturating_add(size) > max_body {
            return Err(ParseError::BodyTooLarge);
        }
        let available = buf.len() - at;
        if available < size.saturating_add(2) {
            if available > size && buf[at + size] != b'\r' {
                return Err(ParseError::BadChunk);
            }
            return Ok(None);
        }
        if &buf[at + size..at + size + 2] != b"\r\n" {
            return Err(ParseError::BadChunk);
        }
        body.extend_from_slice(&buf[at..at + size]);
        at += size + 2;
    }
}
