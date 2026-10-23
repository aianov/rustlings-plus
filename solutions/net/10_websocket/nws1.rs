// Рукопожатие WebSocket гоняет по HTTP две строки в base64: случайный ключ клиента и ответ сервера. Готового
// кодировщика у нас нет, и это не страшно: base64 это четыре печатных символа вместо трёх байтов. Если длина входа не
// кратна трём, последняя четвёрка добивается знаками `=`.
// TODO: Допиши base64_encode в стандартном алфавите (ALPHABET уже есть). Хвост из одного или двух байтов не должен потеряться.

#![allow(dead_code)]
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(data: &[u8]) -> String {
        let mut bits = String::new();
        for byte in data {
            bits.push_str(&format!("{byte:08b}"));
        }
        let mut out = String::new();
        for six in bits.as_bytes().chunks(6) {
            let mut text = String::from_utf8(six.to_vec()).unwrap();
            while text.len() < 6 {
                text.push('0');
            }
            out.push(ALPHABET[usize::from_str_radix(&text, 2).unwrap()] as char);
        }
        while out.len() % 4 != 0 {
            out.push('=');
        }
        out
    }

    #[test]
    fn standard_vectors() {
        for (plain, encoded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64_encode(plain.as_bytes()), encoded, "кодирование {plain:?}");
        }
    }

    #[test]
    fn websocket_sample_nonce() {
        assert_eq!(base64_encode(b"the sample nonce"), "dGhlIHNhbXBsZSBub25jZQ==", "пример ключа из RFC 6455: 16 байт дают 24 символа с `==` в конце");
    }

    #[test]
    fn plus_and_slash_are_in_the_alphabet() {
        assert_eq!(base64_encode(&[0xfb, 0xff, 0xbf]), "+/+/", "индексы 62 и 63 это `+` и `/`");
        assert_eq!(base64_encode(&[0x00, 0x00, 0x00]), "AAAA");
        assert_eq!(base64_encode(&[0xff, 0xff, 0xff]), "////");
    }

    #[test]
    fn every_length_matches_the_slow_reference() {
        let mut seed = 0x9e3779b97f4a7c15u64;
        for len in 0..80usize {
            let data: Vec<u8> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed as u8
                })
                .collect();
            let encoded = base64_encode(&data);
            assert_eq!(encoded.len(), len.div_ceil(3) * 4, "длина результата для {len} байт");
            assert_eq!(encoded, reference(&data), "вход из {len} байт: {data:?}");
        }
    }

    #[test]
    fn all_byte_values() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_encode(&data), reference(&data));
    }
}
