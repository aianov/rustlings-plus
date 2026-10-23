const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let byte = |at: usize| u32::from(chunk.get(at).copied().unwrap_or(0));
        let group = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for index in 0..=chunk.len() {
            let six = (group >> (18 - 6 * index)) & 63;
            out.push(char::from(ALPHABET[six as usize]));
        }
    }
    out
}

fn value(symbol: u8) -> Option<u32> {
    ALPHABET
        .iter()
        .position(|known| *known == symbol)
        .and_then(|at| u32::try_from(at).ok())
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut group = 0u32;
        for (index, symbol) in chunk.iter().enumerate() {
            group |= value(*symbol)? << (18 - 6 * index);
        }
        let bytes = chunk.len() * 6 / 8;
        let spare = (1u32 << (24 - 8 * bytes)) - 1;
        if group & spare != 0 {
            return None;
        }
        out.extend_from_slice(&group.to_be_bytes()[1..=bytes]);
    }
    Some(out)
}
