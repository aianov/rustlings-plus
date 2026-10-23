const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base64Error {
    BadLength,
    BadChar(u8),
    BadPadding,
    NonZeroTail,
}

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64Error::BadLength);
    }
    let groups = bytes.len() / 4;
    let mut out = Vec::with_capacity(groups * 3);
    for (index, quad) in bytes.chunks(4).enumerate() {
        let pad = quad.iter().rev().take_while(|&&b| b == b'=').count();
        if pad > 2 || (pad > 0 && index + 1 != groups) {
            return Err(Base64Error::BadPadding);
        }
        let mut n: u32 = 0;
        for (i, &b) in quad.iter().enumerate() {
            let value = if i < 4 - pad {
                match ALPHABET.iter().position(|&a| a == b) {
                    Some(p) => p as u32,
                    None => return Err(Base64Error::BadChar(b)),
                }
            } else {
                0
            };
            n = (n << 6) | value;
        }
        let used = 3 - pad;
        if n & ((1u32 << (8 * (3 - used))) - 1) != 0 {
            return Err(Base64Error::NonZeroTail);
        }
        for i in 0..used {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Ok(out)
}
