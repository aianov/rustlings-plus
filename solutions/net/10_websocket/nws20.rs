// Свой клиент нужен нам не из любопытства: сторонние клиенты в тестах курса запрещены, а проверять сервер как-то надо.
// Клиент делает зеркальное серверу: генерирует случайный ключ (16 байт в base64), отправляет запрос на апгрейд и
// проверяет ответ. Проверять ответ нужно строго: если сервер ответил 101, но с чужим Accept, это не наш сервер.
// TODO: Допиши new_key, client_request и verify_response. Ответ принимается, если статус 101, есть Upgrade и
// Connection, Accept совпадает с тем, что посчитал бы сервер, и нет расширений и подпротоколов, о которых мы не просили.

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

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn accept_key(client_key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(client_key.as_bytes());
    hasher.update(GUID.as_bytes());
    base64_encode(&hasher.finalize())
}

struct MaskGen {
    state: u64,
}

impl MaskGen {
    fn with_seed(seed: u64) -> MaskGen {
        MaskGen { state: seed | 1 }
    }

    fn next_key(&mut self) -> [u8; 4] {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let mixed = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        ((mixed >> 32) as u32).to_be_bytes()
    }
}

fn new_key(keys: &mut MaskGen) -> String {
    let mut raw = Vec::with_capacity(16);
    for _ in 0..4 {
        raw.extend_from_slice(&keys.next_key());
    }
    base64_encode(&raw)
}

fn client_request(host: &str, path: &str, key: &str, origin: Option<&str>) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let mut out = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n");
    if let Some(origin) = origin {
        out.push_str(&format!("Origin: {origin}\r\n"));
    }
    out.push_str("\r\n");
    out
}

fn verify_response(head: &str, key: &str) -> Result<(), String> {
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or("");
    let mut parts = status.splitn(3, ' ');
    if parts.next() != Some("HTTP/1.1") || parts.next() != Some("101") {
        return Err(format!("ожидали 101 Switching Protocols, пришло {status:?}"));
    }
    let headers: Vec<(String, String)> = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let all = |name: &str| -> Vec<&str> { headers.iter().filter(|(n, _)| n == name).map(|(_, v)| v.as_str()).collect() };
    let has_token = |name: &str, token: &str| all(name).iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token));
    if !has_token("upgrade", "websocket") {
        return Err("нет Upgrade: websocket".to_string());
    }
    if !has_token("connection", "upgrade") {
        return Err("нет Connection: Upgrade".to_string());
    }
    let accepts = all("sec-websocket-accept");
    if accepts.len() != 1 || accepts[0] != accept_key(key) {
        return Err("Sec-WebSocket-Accept не совпал с ожидаемым".to_string());
    }
    if !all("sec-websocket-extensions").is_empty() {
        return Err("сервер включил расширение, о котором мы не просили".to_string());
    }
    if !all("sec-websocket-protocol").is_empty() {
        return Err("сервер выбрал подпротокол, о котором мы не просили".to_string());
    }
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
    const RFC_ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

    fn response(lines: &[&str]) -> String {
        let mut out = lines.join("\r\n");
        out.push_str("\r\n\r\n");
        out
    }

    fn good_response() -> String {
        response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}")])
    }

    fn header_of<'a>(request: &'a str, name: &str) -> Vec<&'a str> {
        request
            .split("\r\n")
            .skip(1)
            .take_while(|l| !l.is_empty())
            .filter_map(|l| l.split_once(':'))
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
            .collect()
    }

    #[test]
    fn keys_are_sixteen_random_bytes_in_base64() {
        let mut keys = MaskGen::with_seed(77);
        let a = new_key(&mut keys);
        let b = new_key(&mut keys);
        assert_ne!(a, b, "каждое соединение получает свой ключ");
        for key in [&a, &b] {
            assert_eq!(key.len(), 24, "16 байт в base64 это 24 символа");
            assert_eq!(base64_decode(key).unwrap().len(), 16, "ключ должен раскодироваться ровно в 16 байт");
        }
    }

    #[test]
    fn request_has_every_required_header() {
        let req = client_request("chat.example:8080", "/ws?token=abc", RFC_KEY, None);
        assert!(req.starts_with("GET /ws?token=abc HTTP/1.1\r\n"), "{req:?}");
        assert!(req.ends_with("\r\n\r\n"), "запрос заканчивается пустой строкой");
        assert_eq!(header_of(&req, "host"), vec!["chat.example:8080"], "Host с портом");
        assert_eq!(header_of(&req, "upgrade"), vec!["websocket"]);
        assert_eq!(header_of(&req, "connection"), vec!["Upgrade"]);
        assert_eq!(header_of(&req, "sec-websocket-key"), vec![RFC_KEY]);
        assert_eq!(header_of(&req, "sec-websocket-version"), vec!["13"]);
        assert!(header_of(&req, "origin").is_empty(), "Origin не просили, значит его нет");
    }

    #[test]
    fn origin_when_asked_and_default_path() {
        let req = client_request("x", "", RFC_KEY, Some("https://app.example"));
        assert!(req.starts_with("GET / HTTP/1.1\r\n"), "пустой путь это `/`");
        assert_eq!(header_of(&req, "origin"), vec!["https://app.example"]);
    }

    #[test]
    fn our_server_side_check_accepts_our_request() {
        let mut keys = MaskGen::with_seed(1);
        let key = new_key(&mut keys);
        let req = client_request("h", "/", &key, None);
        let sent = header_of(&req, "sec-websocket-key")[0].to_string();
        assert_eq!(base64_decode(&sent).unwrap().len(), 16);
    }

    #[test]
    fn good_response_is_accepted() {
        assert_eq!(verify_response(&good_response(), RFC_KEY), Ok(()));
        let shouting = response(&["HTTP/1.1 101 Switching Protocols", "upgrade: WebSocket", "connection: keep-alive, Upgrade", &format!("sec-websocket-accept: {RFC_ACCEPT}")]);
        assert_eq!(verify_response(&shouting, RFC_KEY), Ok(()), "имена заголовков без учёта регистра, Connection это список");
        let extra = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}"), "Server: x", "Date: today"]);
        assert_eq!(verify_response(&extra, RFC_KEY), Ok(()), "посторонние заголовки не мешают");
    }

    #[test]
    fn wrong_status_is_rejected() {
        for line in ["HTTP/1.1 200 OK", "HTTP/1.1 400 Bad Request", "HTTP/1.1 426 Upgrade Required", "HTTP/1.1 1010 No", "HTTP/1.0 101 Switching Protocols", "garbage"] {
            let head = response(&[line, "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}")]);
            assert!(verify_response(&head, RFC_KEY).is_err(), "статус {line:?} не годится");
        }
    }

    #[test]
    fn missing_or_wrong_headers_are_rejected() {
        let no_upgrade = response(&["HTTP/1.1 101 Switching Protocols", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}")]);
        assert!(verify_response(&no_upgrade, RFC_KEY).is_err(), "нет Upgrade");
        let no_connection = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}")]);
        assert!(verify_response(&no_connection, RFC_KEY).is_err(), "нет Connection");
        let no_accept = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade"]);
        assert!(verify_response(&no_accept, RFC_KEY).is_err(), "нет Accept: ответил кто-то, кто не понял рукопожатие");
        let other_protocol = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: h2c", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}")]);
        assert!(verify_response(&other_protocol, RFC_KEY).is_err());
    }

    #[test]
    fn accept_must_match_our_key() {
        assert!(verify_response(&good_response(), "AAAAAAAAAAAAAAAAAAAAAA==").is_err(), "Accept посчитан для другого ключа: это ответ из кеша или чужого соединения");
        let bad = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", "Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo"]);
        assert!(verify_response(&bad, RFC_KEY).is_err(), "без последнего `=` это уже другая строка");
        let two = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}"), "Sec-WebSocket-Accept: other"]);
        assert!(verify_response(&two, RFC_KEY).is_err(), "два Accept: неоднозначность");
    }

    #[test]
    fn unrequested_extensions_and_protocols_are_rejected() {
        let ext = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}"), "Sec-WebSocket-Extensions: permessage-deflate"]);
        assert!(verify_response(&ext, RFC_KEY).is_err(), "сжатие, о котором не просили, сломает разбор фреймов: RSV1 будет не нулём");
        let proto = response(&["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", &format!("Sec-WebSocket-Accept: {RFC_ACCEPT}"), "Sec-WebSocket-Protocol: chat"]);
        assert!(verify_response(&proto, RFC_KEY).is_err());
    }
}
