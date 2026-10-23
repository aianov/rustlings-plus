
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            let v = ALPHABET.iter().position(|a| a == c)? as u32;
            n |= v << (18 - 6 * i);
        }
        let bytes = chunk.len() * 6 / 8;
        if n & ((1u32 << (24 - 8 * bytes)) - 1) != 0 {
            return None;
        }
        out.extend_from_slice(&n.to_be_bytes()[1..1 + bytes]);
    }
    Some(out)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_4648_vectors_without_padding() {
        let vectors = [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")];
        for (plain, encoded) in vectors {
            assert_eq!(b64_encode(plain.as_bytes()), encoded, "кодирование {plain:?}");
            assert_eq!(b64_decode(encoded), Some(plain.as_bytes().to_vec()), "декодирование {encoded:?}");
        }
    }

    #[test]
    fn url_alphabet_is_used() {
        assert_eq!(b64_encode(&[0xfb, 0xff, 0xbf]), "-_-_", "значения 62 и 63 это `-` и `_`, а не `+` и `/`");
        assert_eq!(b64_decode("-_-_"), Some(vec![0xfb, 0xff, 0xbf]), "и обратно");
    }

    #[test]
    fn every_length_roundtrips() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)), Some(data), "длина {len}");
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(b64_decode(&b64_encode(&all)), Some(all), "все значения байта");
    }

    #[test]
    fn strict_decoder_rejects_everything_else() {
        for bad in ["Zg==", "Zm8=", "Zm+v", "Zm/v", "Zm 9v", "Zm9v\n", "Z", "Zm9vY", "Zh", "Zm9", "Zm9vYmF", "Ж", "Zg=", "====" ] {
            assert_eq!(b64_decode(bad), None, "{bad:?} должно отклоняться");
        }
    }
}
