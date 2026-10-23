// Ключ приходит от клиента строкой, и сервер обязан убедиться, что это настоящий base64 ровно на 16 байт. Доверять
// нельзя: лишний пробел, `=` в середине, символ не из алфавита или ненулевые «хвостовые» биты в последнем символе
// это уже не ключ, а мусор.
// TODO: Допиши base64_decode строго: длина кратна четырём, `=` только в конце (не больше двух), символы только из
// алфавита, неиспользуемые биты последнего символа нулевые. Любое нарушение это Err.

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Base64Error {
    BadLength,
    BadChar(u8),
    BadPadding,
    NonZeroTail,
}

fn base64_decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    let _ = text;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_vectors() {
        for (plain, encoded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            let decoded = base64_decode(encoded).unwrap_or_else(|e| panic!("{encoded:?} это правильный base64, а получили {e:?}"));
            assert_eq!(decoded, plain.as_bytes(), "раскодирование {encoded:?}");
        }
    }

    #[test]
    fn websocket_sample_nonce() {
        assert_eq!(base64_decode("dGhlIHNhbXBsZSBub25jZQ==").unwrap(), b"the sample nonce");
    }

    #[test]
    fn plus_and_slash() {
        assert_eq!(base64_decode("+/+/").unwrap(), vec![0xfb, 0xff, 0xbf]);
    }

    #[test]
    fn roundtrip_for_every_length() {
        let mut seed = 0x2545f4914f6cdd1du64;
        for len in 0..70 {
            let data: Vec<u8> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed as u8
                })
                .collect();
            let encoded = base64_encode(&data);
            assert_eq!(base64_decode(&encoded).unwrap(), data, "туда и обратно для {len} байт: {encoded}");
        }
    }

    #[test]
    fn garbage_is_rejected() {
        let bad = [
            ("Zg=", "длина не кратна четырём"),
            ("Zg", "нет паддинга"),
            ("Z===", "три знака `=`"),
            ("====", "одни знаки `=`"),
            ("Zg=a", "символ после `=`"),
            ("=Zg=", "`=` в начале"),
            ("Zg==Zg==", "паддинг в середине строки"),
            ("Zh==", "ненулевые лишние биты в последнем символе"),
            ("Zm9=", "ненулевые лишние биты в последнем символе"),
            ("Zm 9", "пробел внутри"),
            ("Zm9v\n", "перевод строки в конце"),
            ("Zm-v", "url-safe алфавит не наш"),
            ("Zm_v", "url-safe алфавит не наш"),
            ("éééé", "не ASCII"),
        ];
        for (text, why) in bad {
            assert!(base64_decode(text).is_err(), "{text:?} надо отвергнуть: {why}");
        }
    }
}
