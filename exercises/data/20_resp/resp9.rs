// Заголовок `$4000000000\r\n` занимает тринадцать байтов, а парсер, который честно готовится принять
// четыре гигабайта, под них уже выделил память. Тот же трюк с массивами (`*999999999`) и с вложенностью
// (миллион раз `*1`, пока хватит стека). Ответ может прийти от взломанного или сломанного сервера.
// TODO: Добавь проверку пределов из `Limits`: размер значения, число элементов, глубина вложенности.
// Нарушение должно обнаруживаться сразу по заголовку, не дожидаясь тела и не уходя в рекурсию.

use bytes::{Buf, BytesMut};

#[derive(Debug, PartialEq)]
enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Value>),
    NullArray,
}

#[derive(Debug, PartialEq)]
enum ProtoError {
    BadType(u8),
    BadNumber,
    BadLength,
    MissingCrlf,
    BadUtf8,
    TooLarge,
    TooDeep,
}

struct Limits {
    max_bulk: usize,
    max_array: usize,
    max_depth: usize,
}

fn decode(src: &mut BytesMut, limits: &Limits) -> Result<Option<Value>, ProtoError> {
    match parse_at(src, 0, 0, limits)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

fn parse_at(buf: &[u8], pos: usize, depth: usize, limits: &Limits) -> Result<Option<(Value, usize)>, ProtoError> {
    let Some(rel) = buf.get(pos..).and_then(|rest| rest.windows(2).position(|w| w == b"\r\n")) else {
        return Ok(None);
    };
    let end = pos + rel;
    let after = end + 2;
    let kind = buf[pos];
    if !matches!(kind, b'+' | b'-' | b':' | b'$' | b'*') {
        return Err(ProtoError::BadType(kind));
    }
    let line = std::str::from_utf8(&buf[pos + 1..end]).map_err(|_| ProtoError::BadUtf8)?;
    match kind {
        b'+' => Ok(Some((Value::Simple(line.to_string()), after))),
        b'-' => Ok(Some((Value::Error(line.to_string()), after))),
        b':' => Ok(Some((Value::Int(line.parse().map_err(|_| ProtoError::BadNumber)?), after))),
        b'$' => {
            let len: i64 = line.parse().map_err(|_| ProtoError::BadNumber)?;
            if len == -1 {
                return Ok(Some((Value::Null, after)));
            }
            let len = usize::try_from(len).map_err(|_| ProtoError::BadLength)?;
            let body_end = after + len;
            for (i, expected) in [b'\r', b'\n'].iter().enumerate() {
                match buf.get(body_end + i) {
                    None => return Ok(None),
                    Some(b) if b == expected => {}
                    Some(_) => return Err(ProtoError::MissingCrlf),
                }
            }
            Ok(Some((Value::Bulk(buf[after..body_end].to_vec()), body_end + 2)))
        }
        _ => {
            let n: i64 = line.parse().map_err(|_| ProtoError::BadNumber)?;
            if n == -1 {
                return Ok(Some((Value::NullArray, after)));
            }
            let n = usize::try_from(n).map_err(|_| ProtoError::BadLength)?;
            let mut items = Vec::new();
            let mut next = after;
            for _ in 0..n {
                let Some((item, end_of_item)) = parse_at(buf, next, depth + 1, limits)? else {
                    return Ok(None);
                };
                items.push(item);
                next = end_of_item;
            }
            Ok(Some((Value::Array(items), next)))
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits { max_bulk: 16, max_array: 8, max_depth: 4 }
    }

    fn run(wire: &[u8]) -> Result<Option<Value>, ProtoError> {
        let mut src = BytesMut::from(wire);
        decode(&mut src, &limits())
    }

    #[test]
    fn huge_bulk_header_is_refused_at_once() {
        assert_eq!(run(b"$17\r\n"), Err(ProtoError::TooLarge), "заголовок объявляет 17 байтов при пределе 16, ждать тело незачем");
        assert_eq!(run(b"$4000000000\r\n"), Err(ProtoError::TooLarge), "четыре гигабайта: отказ по одному заголовку");
    }

    #[test]
    fn bulk_at_the_limit_is_fine() {
        let mut wire = b"$16\r\n".to_vec();
        wire.extend_from_slice(&[b'x'; 16]);
        wire.extend_from_slice(b"\r\n");
        assert_eq!(run(&wire), Ok(Some(Value::Bulk(vec![b'x'; 16]))), "ровно по пределу пропускаем");
    }

    #[test]
    fn huge_array_header_is_refused_at_once() {
        assert_eq!(run(b"*9\r\n"), Err(ProtoError::TooLarge), "девять элементов при пределе восемь");
        assert_eq!(run(b"*999999999\r\n"), Err(ProtoError::TooLarge), "миллиард элементов: отказ до разбора элементов");
    }

    #[test]
    fn array_at_the_limit_is_fine() {
        let wire = b"*8\r\n:1\r\n:2\r\n:3\r\n:4\r\n:5\r\n:6\r\n:7\r\n:8\r\n";
        match run(wire) {
            Ok(Some(Value::Array(items))) => assert_eq!(items.len(), 8, "восемь элементов"),
            other => panic!("массив ровно по пределу должен разбираться, а получено {other:?}"),
        }
    }

    #[test]
    fn depth_limit() {
        let ok = b"*1\r\n*1\r\n*1\r\n*1\r\n:1\r\n";
        assert!(matches!(run(ok), Ok(Some(Value::Array(_)))), "четыре вложенных массива при пределе четыре проходят");
        let too_deep = b"*1\r\n*1\r\n*1\r\n*1\r\n*1\r\n:1\r\n";
        assert_eq!(run(too_deep), Err(ProtoError::TooDeep), "пятый уровень лишний");
    }

    #[test]
    fn deep_nesting_does_not_blow_the_stack() {
        let wire = b"*1\r\n".repeat(200_000);
        assert_eq!(run(&wire), Err(ProtoError::TooDeep), "двести тысяч вложенных массивов: отказ на пятом, а не переполнение стека");
    }

    #[test]
    fn limits_apply_to_nested_values() {
        assert_eq!(run(b"*2\r\n:1\r\n$99\r\n"), Err(ProtoError::TooLarge), "большой bulk внутри массива тоже заметен по заголовку");
    }
}
