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
}

fn decode(src: &mut BytesMut) -> Result<Option<Value>, ProtoError> {
    match parse_at(src, 0)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

fn parse_at(buf: &[u8], pos: usize) -> Result<Option<(Value, usize)>, ProtoError> {
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
                let Some((item, end_of_item)) = parse_at(buf, next)? else {
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

    fn err(wire: &[u8]) -> ProtoError {
        let mut src = BytesMut::from(wire);
        match decode(&mut src) {
            Err(e) => e,
            Ok(v) => panic!("на {:?} ожидалась ошибка протокола, а получено {v:?}", String::from_utf8_lossy(wire)),
        }
    }

    #[test]
    fn unknown_type() {
        assert_eq!(err(b"!x\r\n"), ProtoError::BadType(b'!'), "восклицательный знак не тип RESP");
        assert_eq!(err(b"\r\n"), ProtoError::BadType(b'\r'), "пустая строка: первый байт сам CR");
    }

    #[test]
    fn bad_numbers() {
        assert_eq!(err(b":12ab\r\n"), ProtoError::BadNumber, "в числе буквы");
        assert_eq!(err(b":99999999999999999999\r\n"), ProtoError::BadNumber, "число не влезает в i64");
        assert_eq!(err(b"$abc\r\n"), ProtoError::BadNumber, "длина не число");
        assert_eq!(err(b"*x\r\n"), ProtoError::BadNumber, "число элементов не число");
    }

    #[test]
    fn bad_lengths() {
        assert_eq!(err(b"$-2\r\n"), ProtoError::BadLength, "отрицательная длина, кроме -1, не бывает");
        assert_eq!(err(b"*-5\r\n"), ProtoError::BadLength, "и для массива тоже");
        assert_eq!(err(b"$-9223372036854775808\r\n"), ProtoError::BadLength, "самое отрицательное число: приведение к usize дало бы гигантскую длину");
    }

    #[test]
    fn missing_crlf_after_body() {
        assert_eq!(err(b"$3\r\nabcde"), ProtoError::MissingCrlf, "после трёх байтов тела вместо CRLF идёт «de»");
        assert_eq!(err(b"$3\r\nabc\rX"), ProtoError::MissingCrlf, "после тела CR и не LF");
        assert_eq!(err(b"*1\r\n$1\r\nabX\r\n"), ProtoError::MissingCrlf, "то же внутри массива");
    }

    #[test]
    fn bad_utf8_in_status() {
        assert_eq!(err(b"+\xff\xfe\r\n"), ProtoError::BadUtf8, "статус обязан быть текстом");
    }

    #[test]
    fn missing_data_is_not_an_error() {
        for wire in [&b"$3\r\nab"[..], b"$3\r\nabc", b"$3\r\nabc\r", b"*2\r\n:1\r\n", b":12", b"+OK\r"] {
            let mut src = BytesMut::from(wire);
            assert_eq!(decode(&mut src), Ok(None), "данных пока мало: {:?}", String::from_utf8_lossy(wire));
            assert_eq!(src.len(), wire.len(), "буфер не тронут");
        }
    }

    #[test]
    fn nothing_panics_on_garbage() {
        let alphabet = b"+-:$*0123456789\r\n a\xff";
        let mut x: u64 = 12345;
        for _ in 0..4000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let len = (x >> 60) as usize + 1;
            let mut wire = Vec::new();
            let mut y = x;
            for _ in 0..len {
                y = y.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                wire.push(alphabet[(y >> 33) as usize % alphabet.len()]);
            }
            let mut src = BytesMut::from(&wire[..]);
            let _ = decode(&mut src);
        }
    }
}
