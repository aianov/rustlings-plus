// Ответ сервера состоит из текстового заголовка и тела из произвольных байт. Если собирать его через
// `String`, тело придётся пропустить через UTF-8 и потерять (или испортить) всё, что не текст. Правильно писать
// сразу в `Vec<u8>`: он реализует `io::Write`, и `write!` работает с ним как с файлом. Для текстовых
// результатов есть `fmt::Write`, и `write!` в `String` не создаёт промежуточных строк на каждую часть.
// Лишние временные строки (форматирование в строку ради того, чтобы её тут же добавить) тест считает.
// TODO: Допиши `response` и `render_ids`, не заводя временных строк.
use std::fmt::Write as _;
use std::io::Write as _;

pub fn response(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    todo!("HTTP/1.1 {status} {reason}, Content-Length, пустая строка, тело")
}

pub fn render_ids(ids: &[u64]) -> String {
    todo!("[1, 2, 3]")
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
