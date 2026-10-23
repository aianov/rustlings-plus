use std::fmt::Write as _;
use std::io::Write as _;

pub fn response(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(48 + body.len());
    write!(out, "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n\r\n", body.len()).expect("запись в Vec не может провалиться");
    out.extend_from_slice(body);
    out
}

pub fn render_ids(ids: &[u64]) -> String {
    let mut out = String::from("[");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "{id}").expect("запись в String не может провалиться");
    }
    out.push(']');
    out
}

fn main() {
    print!("{}", String::from_utf8_lossy(&response(200, "OK", b"hi")));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("byt19.rs");

    #[test]
    fn a_text_response_is_exact() {
        assert_eq!(response(200, "OK", b"hello"), b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec());
        assert_eq!(response(404, "Not Found", b""), b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec());
    }

    #[test]
    fn a_binary_body_survives_untouched() {
        let body = [0xff, 0xfe, 0x00, 0x80, b'\r', b'\n'];
        let out = response(200, "OK", &body);
        assert!(out.ends_with(&body), "тело изменилось: байты вне UTF-8 нельзя пропускать через String");
        assert!(out.starts_with(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n"));
        assert_eq!(out.len(), 38 + 6);
    }

    #[test]
    fn id_lists_are_rendered() {
        assert_eq!(render_ids(&[]), "[]");
        assert_eq!(render_ids(&[7]), "[7]");
        assert_eq!(render_ids(&[1, 22, 333]), "[1, 22, 333]");
    }

    #[test]
    fn no_temporary_strings_are_built() {
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        assert!(!code.contains("format!("), "format! создаёт временную строку на каждый вызов: пиши прямо в приёмник через write!");
        assert!(!code.contains(".to_string()"), "лишнее преобразование в строку");
        assert!(code.contains("write!"), "ожидался write! в Vec<u8> и в String");
    }
}
