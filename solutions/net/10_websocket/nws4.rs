// Прежде чем отвечать 101, сервер проверяет, что к нему действительно пришли по WebSocket, а не просто открыли
// адрес в браузере. Для людей, которые ткнули в адрес, нужен внятный отказ, для старых клиентов с другой версией
// протокола особый (426 и подсказка, какая версия нужна), для остальных ошибок 400.
// TODO: Допиши check_upgrade: вернуть ключ клиента, если запрос годится, иначе Reject. Порядок проверок: метод
// (405 и `Allow: GET`), версия HTTP (нужна 1.1), заголовки Upgrade и Connection, ровно один Sec-WebSocket-Key,
// версия протокола (426 и `Sec-WebSocket-Version: 13`).

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

#[derive(Debug, Clone)]
struct Req {
    method: String,
    version: String,
    headers: Vec<(String, String)>,
}

impl Req {
    fn get() -> Req {
        Req { method: "GET".to_string(), version: "HTTP/1.1".to_string(), headers: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Reject {
    status: u16,
    message: &'static str,
    headers: Vec<(&'static str, &'static str)>,
}

fn has_token(req: &Req, name: &str, token: &str) -> bool {
    req.all(name).iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token))
}

fn bad(message: &'static str) -> Reject {
    Reject { status: 400, message, headers: Vec::new() }
}

fn check_upgrade(req: &Req) -> Result<String, Reject> {
    if req.method != "GET" {
        return Err(Reject { status: 405, message: "only GET", headers: vec![("Allow", "GET")] });
    }
    if req.version != "HTTP/1.1" {
        return Err(bad("HTTP/1.1 required"));
    }
    if !has_token(req, "upgrade", "websocket") {
        return Err(bad("Upgrade: websocket expected"));
    }
    if !has_token(req, "connection", "upgrade") {
        return Err(bad("Connection: Upgrade expected"));
    }
    let keys = req.all("sec-websocket-key");
    let [key] = keys.as_slice() else {
        return Err(bad("exactly one Sec-WebSocket-Key expected"));
    };
    let key = key.trim();
    match base64_decode(key) {
        Ok(raw) if raw.len() == 16 => {}
        _ => return Err(bad("Sec-WebSocket-Key must be 16 bytes in base64")),
    }
    let versions = req.all("sec-websocket-version");
    if versions.len() != 1 || versions[0].trim() != "13" {
        return Err(Reject { status: 426, message: "unsupported version", headers: vec![("Sec-WebSocket-Version", "13")] });
    }
    Ok(key.to_string())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    fn good() -> Req {
        Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", KEY).with("Sec-WebSocket-Version", "13")
    }

    fn status(req: &Req) -> u16 {
        match check_upgrade(req) {
            Ok(_) => 101,
            Err(r) => r.status,
        }
    }

    #[test]
    fn good_request_returns_the_key() {
        assert_eq!(check_upgrade(&good()).unwrap(), KEY);
    }

    #[test]
    fn key_is_trimmed() {
        let req = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", &format!("  {KEY} ")).with("Sec-WebSocket-Version", "13");
        assert_eq!(check_upgrade(&req).unwrap(), KEY, "пробелы вокруг значения не часть ключа");
    }

    #[test]
    fn only_get() {
        for method in ["POST", "PUT", "HEAD", "get"] {
            let mut req = good();
            req.method = method.to_string();
            let r = check_upgrade(&req).unwrap_err();
            assert_eq!(r.status, 405, "метод {method}");
            assert!(r.headers.iter().any(|(n, v)| *n == "Allow" && *v == "GET"), "405 обязан подсказать Allow: GET");
        }
    }

    #[test]
    fn http_1_0_cannot_upgrade() {
        let mut req = good();
        req.version = "HTTP/1.0".to_string();
        assert_eq!(status(&req), 400);
    }

    #[test]
    fn upgrade_and_connection_are_required() {
        let no_upgrade = Req::get().with("Connection", "Upgrade").with("Sec-WebSocket-Key", KEY).with("Sec-WebSocket-Version", "13");
        assert_eq!(status(&no_upgrade), 400, "без Upgrade это обычный запрос");
        let no_connection = Req::get().with("Upgrade", "websocket").with("Sec-WebSocket-Key", KEY).with("Sec-WebSocket-Version", "13");
        assert_eq!(status(&no_connection), 400, "без Connection: Upgrade прокси срежет заголовок Upgrade");
        let other = Req::get().with("Upgrade", "h2c").with("Connection", "Upgrade").with("Sec-WebSocket-Key", KEY).with("Sec-WebSocket-Version", "13");
        assert_eq!(status(&other), 400, "Upgrade на другой протокол");
    }

    #[test]
    fn key_must_be_exactly_sixteen_bytes() {
        for (key, why) in [
            ("AAAAAAAAAAAAAAAAAAAAAAAA", "24 символа, но это 18 байт"),
            ("AAAAAAAAAAAAAAAA", "12 байт"),
            ("AAAAAAAAAAAAAAAAAAAAAA", "нет паддинга"),
            ("!!!!!!!!!!!!!!!!!!!!!!==", "не base64"),
            ("", "пустой"),
        ] {
            let req = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", key).with("Sec-WebSocket-Version", "13");
            assert_eq!(status(&req), 400, "ключ {key:?}: {why}");
        }
        let valid = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", "AAAAAAAAAAAAAAAAAAAAAA==").with("Sec-WebSocket-Version", "13");
        assert_eq!(status(&valid), 101, "22 символа и `==` это ровно 16 байт");
    }

    #[test]
    fn missing_or_double_key() {
        let none = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Version", "13");
        assert_eq!(status(&none), 400);
        let two = good().with("Sec-WebSocket-Key", KEY);
        assert_eq!(status(&two), 400, "два ключа: непонятно, какому верить");
    }

    #[test]
    fn wrong_version_gets_426_with_a_hint() {
        for version in ["8", "12", "14", "13.0", ""] {
            let req = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", KEY).with("Sec-WebSocket-Version", version);
            let r = check_upgrade(&req).unwrap_err();
            assert_eq!(r.status, 426, "версия {version:?}");
            assert!(r.headers.iter().any(|(n, v)| *n == "Sec-WebSocket-Version" && *v == "13"), "клиент должен узнать, какая версия нужна");
        }
        let missing = Req::get().with("Upgrade", "websocket").with("Connection", "Upgrade").with("Sec-WebSocket-Key", KEY);
        assert_eq!(status(&missing), 426, "версии нет совсем");
    }
}
