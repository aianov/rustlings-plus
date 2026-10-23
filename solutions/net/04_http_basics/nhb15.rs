#[derive(Debug, Clone)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    }
}

impl Response {
    fn new(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Vec::new() }
    }

    fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn body(mut self, body: impl Into<Vec<u8>>) -> Response {
        self.body = body.into();
        self
    }

    fn to_bytes(&self, head_request: bool) -> Vec<u8> {
        let bodyless = self.status < 200 || self.status == 204 || self.status == 304;
        let mut out = format!("HTTP/1.1 {} {}\r\n", self.status, reason(self.status)).into_bytes();
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        let has_length = self.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-length"));
        if !bodyless && !has_length {
            out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if !bodyless && !head_request {
            out.extend_from_slice(&self.body);
        }
        out
    }
}

fn main() {
    let bytes = Response::new(200).header("Content-Type", "text/plain").body("hello").to_bytes(false);
    print!("{}", String::from_utf8_lossy(&bytes));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bytes_of_a_simple_response() {
        let bytes = Response::new(200).header("Content-Type", "text/plain").body("hello").to_bytes(false);
        assert_eq!(String::from_utf8_lossy(&bytes), "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello", "строка статуса, заголовки по порядку, Content-Length, пустая строка, тело");
    }

    #[test]
    fn empty_body_still_has_length_zero() {
        let bytes = Response::new(404).to_bytes(false);
        assert_eq!(bytes, b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n", "без Content-Length клиент не поймёт, что тела нет");
    }

    #[test]
    fn user_length_is_not_duplicated() {
        let bytes = Response::new(200).header("content-length", "5").body("hello").to_bytes(false);
        let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        assert_eq!(text.matches("content-length").count(), 1, "два Content-Length это уже контрабанда: {text:?}");
    }

    #[test]
    fn bodyless_statuses_have_neither_length_nor_body() {
        for code in [204, 304] {
            let bytes = Response::new(code).body("must not be sent").to_bytes(false);
            let text = String::from_utf8_lossy(&bytes).to_string();
            assert!(!text.contains("must not"), "у {code} тела быть не может: {text:?}");
            assert!(!text.to_ascii_lowercase().contains("content-length"), "у {code} не должно быть Content-Length: {text:?}");
            assert!(text.ends_with("\r\n\r\n"), "ответ заканчивается пустой строкой: {text:?}");
        }
    }

    #[test]
    fn head_request_keeps_length_drops_body() {
        let bytes = Response::new(200).body("hello").to_bytes(true);
        assert_eq!(String::from_utf8_lossy(&bytes), "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n", "на HEAD длина остаётся такой, как у GET, а тела нет");
    }

    #[test]
    fn unknown_status_has_empty_reason() {
        let bytes = Response::new(299).to_bytes(false);
        assert!(bytes.starts_with(b"HTTP/1.1 299 \r\n"), "пробел после кода обязателен даже при пустой причине: {:?}", String::from_utf8_lossy(&bytes));
    }

    #[test]
    fn binary_body_is_untouched() {
        let body = vec![0u8, 13, 10, 13, 10, 255];
        let bytes = Response::new(200).body(body.clone()).to_bytes(false);
        assert!(bytes.ends_with(&body));
        assert!(String::from_utf8_lossy(&bytes).contains("Content-Length: 6\r\n"), "длина считается в байтах");
    }
}
