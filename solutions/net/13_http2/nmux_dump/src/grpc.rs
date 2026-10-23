use std::time::Duration;

pub struct Messages {
    buf: Vec<u8>,
}

impl Messages {
    pub fn new() -> Messages {
        Messages { buf: Vec::new() }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        let mut pos = 0;
        while self.buf.len() - pos >= 5 {
            if self.buf[pos] != 0 {
                return Err("сжатые сообщения не поддерживаются".to_string());
            }
            let length = u32::from_be_bytes([self.buf[pos + 1], self.buf[pos + 2], self.buf[pos + 3], self.buf[pos + 4]]) as usize;
            if self.buf.len() - pos - 5 < length {
                break;
            }
            out.push(self.buf[pos + 5..pos + 5 + length].to_vec());
            pos += 5 + length;
        }
        self.buf.drain(..pos);
        Ok(out)
    }
}

pub fn parse_timeout(text: &str) -> Option<Duration> {
    let unit = text.chars().last()?;
    let digits = &text[..text.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        'H' => Duration::from_secs(n * 3600),
        'M' => Duration::from_secs(n * 60),
        'S' => Duration::from_secs(n),
        'm' => Duration::from_millis(n),
        'u' => Duration::from_micros(n),
        'n' => Duration::from_nanos(n),
        _ => return None,
    })
}

pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
            out.push(u8::from_str_radix(hex, 16).unwrap_or(0));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
