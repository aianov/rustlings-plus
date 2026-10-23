// Сервер доказывает клиенту, что он понял рукопожатие, не отправкой «да», а вычислением: берёт присланный ключ,
// приписывает к нему фиксированную строку GUID, считает sha1 и кодирует результат в base64. Это и есть
// Sec-WebSocket-Accept. Никакой секретности тут нет, смысл только в том, чтобы обычный HTTP-сервер или кеш случайно
// не ответил «подходящим» значением.
// TODO: Допиши accept_key. Хэш берёшь из крейта `sha1` (`Sha1::new`, `update`, `finalize`), а base64 уже готов.

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

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn accept_key(client_key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(client_key.as_bytes());
    hasher.update(GUID.as_bytes());
    base64_encode(&hasher.finalize())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_from_rfc_6455() {
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", "пример из RFC 6455, раздел 1.3");
    }

    #[test]
    fn another_known_pair() {
        assert_eq!(accept_key("x3JJHMbDL1EzLkh9GBhXDw=="), "HSmrc0sMlYUkAGmm5OPpG2HaGWk=");
    }

    #[test]
    fn digest_is_raw_bytes_not_hex() {
        let accept = accept_key("AQIDBAUGBwgJCgsMDQ4PEA==");
        assert_eq!(accept.len(), 28, "sha1 это 20 байт, в base64 они дают ровно 28 символов; хэш в виде hex-текста дал бы 56");
        assert!(accept.ends_with('='), "20 байт не кратны трём, в конце один `=`");
    }

    #[test]
    fn depends_on_the_key_and_is_stable() {
        let a = accept_key("AAAAAAAAAAAAAAAAAAAAAA==");
        let b = accept_key("AAAAAAAAAAAAAAAAAAAAAQ==");
        assert_ne!(a, b, "разные ключи дают разные ответы");
        assert_eq!(a, accept_key("AAAAAAAAAAAAAAAAAAAAAA=="), "ответ детерминирован");
    }
}
