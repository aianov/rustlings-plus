// Токены и подписи ходят в заголовках и адресах, а обычный base64 содержит `+`, `/` и `=`, которые
// ломают URL. Для этого есть base64url: другой алфавит (`-` и `_` вместо `+` и `/`) и без `=` в конце.
// Крейт тащить не будем, это двадцать строк. Цель: `b64_encode` и `b64_decode` по RFC 4648 без
// паддинга. Декодер строгий: чужой алфавит, `=`, пробелы, невозможная длина и «хвостовые» ненулевые
// биты это отказ (`None`), потому что у токена не должно быть двух записей одного и того же.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64_encode(data: &[u8]) -> String {
    todo!()
}

pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    todo!()
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
