// Парсер ответов давно работает в проде и ни разу не подводил. Потом один пользователь сохранил в
// профиль «о себе» в три абзаца, и после показа его профиля клиент начал раздавать чужие ответы не
// на те команды. Тесты ниже воспроизводят это. Найди, что парсер принимает за границу значения, и почини.

use bytes::{Buf, BytesMut};

#[derive(Debug, PartialEq)]
enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
}

#[derive(Debug, PartialEq)]
struct Bad;

fn find_crlf(buf: &[u8], from: usize) -> Option<usize> {
    buf.get(from..)?.windows(2).position(|w| w == b"\r\n").map(|p| from + p)
}

fn decode(src: &mut BytesMut) -> Result<Option<Value>, Bad> {
    let Some(first) = find_crlf(src, 0) else {
        return Ok(None);
    };
    let line = std::str::from_utf8(src.get(1..first).ok_or(Bad)?).map_err(|_| Bad)?.to_string();
    match src[0] {
        b'+' => {
            src.advance(first + 2);
            Ok(Some(Value::Simple(line)))
        }
        b'-' => {
            src.advance(first + 2);
            Ok(Some(Value::Error(line)))
        }
        b':' => {
            let n = line.parse().map_err(|_| Bad)?;
            src.advance(first + 2);
            Ok(Some(Value::Int(n)))
        }
        b'$' => {
            let declared: i64 = line.parse().map_err(|_| Bad)?;
            if declared < 0 {
                src.advance(first + 2);
                return Ok(Some(Value::Null));
            }
            let Some(second) = find_crlf(src, first + 2) else {
                return Ok(None);
            };
            let body = src[first + 2..second].to_vec();
            src.advance(second + 2);
            Ok(Some(Value::Bulk(body)))
        }
        _ => Err(Bad),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(wire: &[u8]) -> Vec<Value> {
        let mut src = BytesMut::from(wire);
        let mut out = Vec::new();
        while let Some(v) = decode(&mut src).expect("корректные данные не должны давать ошибку") {
            out.push(v);
        }
        assert!(src.is_empty(), "после разбора остались лишние байты {:?}: парсер счёл границей кадра не то место", String::from_utf8_lossy(&src));
        out
    }

    #[test]
    fn ordinary_replies_work() {
        let got = all(b"+OK\r\n$3\r\nfoo\r\n:7\r\n$-1\r\n-ERR x\r\n");
        assert_eq!(
            got,
            vec![Value::Simple("OK".into()), Value::Bulk(b"foo".to_vec()), Value::Int(7), Value::Null, Value::Error("ERR x".into())],
            "обычные ответы парсер читал всегда правильно"
        );
    }

    #[test]
    fn value_with_line_breaks_inside() {
        let text = b"line1\r\nline2";
        let wire = [b"$12\r\n".as_slice(), text, b"\r\n:5\r\n"].concat();
        assert_eq!(all(&wire), vec![Value::Bulk(text.to_vec()), Value::Int(5)], "значение с переводом строки должно прийти целиком, а следующий ответ остаться на своём месте");
    }

    #[test]
    fn binary_value() {
        let raw = [0u8, 1, 2, b'\r', b'\n', 255, b'\r', b'\n', 9];
        let wire = [b"$9\r\n".as_slice(), &raw, b"\r\n+AFTER\r\n"].concat();
        assert_eq!(all(&wire), vec![Value::Bulk(raw.to_vec()), Value::Simple("AFTER".into())], "двоичное значение с CRLF внутри");
    }

    #[test]
    fn body_has_not_arrived_yet() {
        let mut src = BytesMut::from(&b"$6\r\nab\r\n"[..]);
        assert_eq!(decode(&mut src), Ok(None), "объявлено 6 байтов тела, а пришло 4 (два символа и перевод строки): значение ещё не дочитано, это не конец");
        assert_eq!(src.len(), 8, "неполный кадр остаётся в буфере");
        src.extend_from_slice(b"cd\r\n");
        assert_eq!(decode(&mut src), Ok(Some(Value::Bulk(b"ab\r\ncd".to_vec()))), "дописали остаток: значение «ab\\r\\ncd»");
    }
}
