#[derive(Debug, PartialEq)]
pub enum HexError {
    OddLength(usize),
    BadChar { index: usize, byte: u8 },
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

pub fn decode_hex(text: &str) -> Result<Vec<u8>, HexError> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(HexError::OddLength(bytes.len()));
    }
    let digit = |index: usize| -> Result<u8, HexError> {
        let byte = bytes[index];
        char::from(byte).to_digit(16).map(|d| d as u8).ok_or(HexError::BadChar { index, byte })
    };
    (0..bytes.len()).step_by(2).map(|i| Ok(digit(i)? << 4 | digit(i + 1)?)).collect()
}

fn main() {
    println!("{}", encode_hex(b"hi"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_is_lowercase_pairs() {
        assert_eq!(encode_hex(b""), "");
        assert_eq!(encode_hex(&[0, 1, 15, 16, 255]), "00010f10ff");
        assert_eq!(encode_hex("Я".as_bytes()), "d0af");
    }

    #[test]
    fn decoding_inverts_encoding() {
        let data: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_hex(&encode_hex(&data)), Ok(data));
        assert_eq!(decode_hex("DEADbeef"), Ok(vec![0xde, 0xad, 0xbe, 0xef]), "заглавные и строчные буквы равноправны");
        assert_eq!(decode_hex(""), Ok(vec![]));
    }

    #[test]
    fn odd_length_is_reported_by_bytes() {
        assert_eq!(decode_hex("abc"), Err(HexError::OddLength(3)));
        assert_eq!(decode_hex("a"), Err(HexError::OddLength(1)));
    }

    #[test]
    fn foreign_characters_are_errors_with_a_position() {
        assert_eq!(decode_hex("0g"), Err(HexError::BadChar { index: 1, byte: b'g' }));
        assert_eq!(decode_hex("zz00"), Err(HexError::BadChar { index: 0, byte: b'z' }));
        assert_eq!(decode_hex("+f"), Err(HexError::BadChar { index: 0, byte: b'+' }), "знак это не цифра");
        assert_eq!(decode_hex("0x"), Err(HexError::BadChar { index: 1, byte: b'x' }));
        assert_eq!(decode_hex("a b0"), Err(HexError::BadChar { index: 1, byte: b' ' }));
    }

    #[test]
    fn multibyte_letters_do_not_panic() {
        assert_eq!(decode_hex("aé"), Err(HexError::OddLength(3)));
        assert_eq!(decode_hex("é"), Err(HexError::BadChar { index: 0, byte: 0xc3 }));
        assert_eq!(decode_hex("ab👋"), Err(HexError::BadChar { index: 2, byte: 0xf0 }));
    }
}
