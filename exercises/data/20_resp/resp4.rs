// Теперь в обратную сторону: от сервера пришли байты, и из них надо достать ответ. Пока считаем,
// что в буфере кадр целиком (недостающие данные будут в следующих задачах). Нужно не только значение,
// но и сколько байтов оно заняло: после него в буфере может лежать следующий ответ.
// TODO: Разбери один ответ с начала буфера. На всё, что не умеешь или что не похоже на RESP, верни None.

#[derive(Debug, PartialEq)]
enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
}

fn parse(buf: &[u8]) -> Option<(Value, usize)> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_error_and_number() {
        assert_eq!(parse(b"+OK\r\n"), Some((Value::Simple("OK".into()), 5)), "статус");
        assert_eq!(parse(b"-ERR no such key\r\n"), Some((Value::Error("ERR no such key".into()), 18)), "ошибка целиком, вместе с видом");
        assert_eq!(parse(b":1000\r\n"), Some((Value::Int(1000), 7)), "число");
        assert_eq!(parse(b":-5\r\n"), Some((Value::Int(-5), 5)), "отрицательное число");
    }

    #[test]
    fn bulk_strings() {
        assert_eq!(parse(b"$5\r\nhello\r\n"), Some((Value::Bulk(b"hello".to_vec()), 11)), "значение и длина кадра с обоими CRLF");
        assert_eq!(parse(b"$0\r\n\r\n"), Some((Value::Bulk(Vec::new()), 6)), "пустое значение");
        assert_eq!(parse(b"$-1\r\n"), Some((Value::Null, 5)), "нет значения");
    }

    #[test]
    fn utf8_bulk_has_byte_length() {
        let wire = "$6\r\nпри\r\n".as_bytes();
        assert_eq!(parse(wire), Some((Value::Bulk("при".as_bytes().to_vec()), 12)), "шесть байтов тела, всего кадр 12 байтов");
    }

    #[test]
    fn reports_how_much_it_used() {
        let wire = b"+OK\r\n:7\r\n$3\r\nfoo\r\n";
        let (first, used) = parse(wire).expect("первый ответ");
        assert_eq!(first, Value::Simple("OK".into()), "первый ответ");
        assert_eq!(used, 5, "первый кадр занимает 5 байтов, остальное принадлежит следующим");
        let (second, used2) = parse(&wire[used..]).expect("второй ответ");
        assert_eq!(second, Value::Int(7), "второй ответ");
        let (third, _) = parse(&wire[used + used2..]).expect("третий ответ");
        assert_eq!(third, Value::Bulk(b"foo".to_vec()), "третий ответ");
    }

    #[test]
    fn unknown_type_is_none() {
        assert_eq!(parse(b"?what\r\n"), None, "первый байт ? не из протокола");
        assert_eq!(parse(b""), None, "пустой буфер");
    }
}
