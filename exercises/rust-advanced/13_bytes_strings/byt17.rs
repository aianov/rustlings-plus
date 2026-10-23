// Хеши, идентификаторы устройств и содержимое пакетов в логах и API записывают шестнадцатеричной строкой,
// по два символа на байт. Кодировать просто, а вот декодировать надо аккуратно: нечётная длина, чужие символы
// и многобайтовые буквы не должны приводить к панике. Срезать строку по два символа (`&text[i..i + 2]`)
// опасно: на букве из двух байт это паника, а `from_str_radix` пропускает знак `+`.
// TODO: Допиши `encode_hex` (строчные буквы) и `decode_hex` (принимает и заглавные). Сначала проверяется
// длина в байтах, потом символы; в ошибке индекс считается по байтам.
#[derive(Debug, PartialEq)]
pub enum HexError {
    OddLength(usize),
    BadChar { index: usize, byte: u8 },
}

pub fn encode_hex(bytes: &[u8]) -> String {
    todo!("по два символа на байт")
}

pub fn decode_hex(text: &str) -> Result<Vec<u8>, HexError> {
    todo!("обратное преобразование")
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
