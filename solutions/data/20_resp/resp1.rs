fn simple(text: &str) -> Option<Vec<u8>> {
    if text.contains(['\r', '\n']) {
        return None;
    }
    Some(format!("+{text}\r\n").into_bytes())
}

fn error(kind: &str, text: &str) -> Option<Vec<u8>> {
    if kind.contains(['\r', '\n']) || text.contains(['\r', '\n']) {
        return None;
    }
    Some(format!("-{kind} {text}\r\n").into_bytes())
}

fn integer(n: i64) -> Vec<u8> {
    format!(":{n}\r\n").into_bytes()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_plus_text_crlf() {
        assert_eq!(simple("OK"), Some(b"+OK\r\n".to_vec()), "статус OK это плюс, текст и CRLF");
        assert_eq!(simple("PONG"), Some(b"+PONG\r\n".to_vec()), "статус PONG");
        assert_eq!(simple(""), Some(b"+\r\n".to_vec()), "пустой статус тоже кадр");
    }

    #[test]
    fn status_with_line_break_is_refused() {
        for bad in ["a\nb", "a\rb", "line\r\n", "\n"] {
            assert_eq!(simple(bad), None, "в статусе {bad:?} есть перевод строки: клиент примет его за конец кадра, а остаток за следующий ответ");
        }
    }

    #[test]
    fn error_has_kind_then_text() {
        assert_eq!(
            error("WRONGTYPE", "Operation against a key holding the wrong kind of value"),
            Some(b"-WRONGTYPE Operation against a key holding the wrong kind of value\r\n".to_vec()),
            "ошибка: минус, вид, пробел, текст, CRLF"
        );
        assert_eq!(error("ERR", "unknown command"), Some(b"-ERR unknown command\r\n".to_vec()), "обычная ошибка ERR");
    }

    #[test]
    fn error_with_line_break_is_refused() {
        assert_eq!(error("ERR", "first\r\nsecond"), None, "перевод строки в тексте ошибки ломает кадр так же, как в статусе");
        assert_eq!(error("ER\nR", "x"), None, "и в виде ошибки тоже");
    }

    #[test]
    fn integers_are_decimal_text() {
        assert_eq!(integer(0), b":0\r\n".to_vec(), "ноль");
        assert_eq!(integer(42), b":42\r\n".to_vec(), "положительное");
        assert_eq!(integer(-1), b":-1\r\n".to_vec(), "отрицательное: минус остаётся в тексте числа");
        assert_eq!(integer(i64::MAX), b":9223372036854775807\r\n".to_vec(), "наибольшее i64");
        assert_eq!(integer(i64::MIN), b":-9223372036854775808\r\n".to_vec(), "наименьшее i64");
    }
}
