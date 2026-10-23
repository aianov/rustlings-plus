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
struct Bad;

fn decode(src: &mut BytesMut) -> Result<Option<Value>, Bad> {
    match parse_at(src, 0)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

fn parse_at(buf: &[u8], pos: usize) -> Result<Option<(Value, usize)>, Bad> {
    let Some(rel) = buf.get(pos..).and_then(|rest| rest.windows(2).position(|w| w == b"\r\n")) else {
        return Ok(None);
    };
    let end = pos + rel;
    let after = end + 2;
    let line = std::str::from_utf8(buf.get(pos + 1..end).ok_or(Bad)?).map_err(|_| Bad)?;
    match buf[pos] {
        b'+' => Ok(Some((Value::Simple(line.to_string()), after))),
        b'-' => Ok(Some((Value::Error(line.to_string()), after))),
        b':' => Ok(Some((Value::Int(line.parse().map_err(|_| Bad)?), after))),
        b'$' => {
            let len: i64 = line.parse().map_err(|_| Bad)?;
            if len < 0 {
                return Ok(Some((Value::Null, after)));
            }
            let len = len as usize;
            if buf.len() < after + len + 2 {
                return Ok(None);
            }
            Ok(Some((Value::Bulk(buf[after..after + len].to_vec()), after + len + 2)))
        }
        b'*' => {
            let n: i64 = line.parse().map_err(|_| Bad)?;
            if n < 0 {
                return Ok(Some((Value::NullArray, after)));
            }
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
        _ => Err(Bad),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(wire: &[u8]) -> Value {
        let mut src = BytesMut::from(wire);
        let v = decode(&mut src).expect("без ошибки").expect("кадр целый");
        assert!(src.is_empty(), "кадр должен быть съеден целиком, а осталось {} байтов", src.len());
        v
    }

    fn b(s: &str) -> Value {
        Value::Bulk(s.as_bytes().to_vec())
    }

    #[test]
    fn flat_array() {
        assert_eq!(one(b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n"), Value::Array(vec![b("SET"), b("k"), b("v")]), "команда SET k v");
    }

    #[test]
    fn empty_and_null_arrays() {
        assert_eq!(one(b"*0\r\n"), Value::Array(vec![]), "пустой массив");
        assert_eq!(one(b"*-1\r\n"), Value::NullArray, "отсутствующий массив");
    }

    #[test]
    fn nested_arrays() {
        let wire = b"*2\r\n*2\r\n:1\r\n:2\r\n*1\r\n+x\r\n";
        assert_eq!(
            one(wire),
            Value::Array(vec![Value::Array(vec![Value::Int(1), Value::Int(2)]), Value::Array(vec![Value::Simple("x".into())])]),
            "массив массивов"
        );
    }

    #[test]
    fn exec_like_mixed_reply() {
        let wire = b"*4\r\n+OK\r\n:1\r\n*2\r\n$1\r\na\r\n$-1\r\n*-1\r\n";
        assert_eq!(
            one(wire),
            Value::Array(vec![Value::Simple("OK".into()), Value::Int(1), Value::Array(vec![b("a"), Value::Null]), Value::NullArray]),
            "внутри массива встречаются любые виды, в том числе отсутствующие значения"
        );
    }

    #[test]
    fn incomplete_array_leaves_buffer_alone() {
        let wire = b"*3\r\n$2\r\nab\r\n*2\r\n:1\r\n:2\r\n+tail\r\n";
        for cut in 0..wire.len() {
            let mut src = BytesMut::from(&wire[..cut]);
            assert_eq!(decode(&mut src), Ok(None), "после {cut} байтов массив ещё не завершён");
            assert_eq!(src.len(), cut, "неполный массив нельзя разбирать по частям: при {cut} байтах буфер изменился (теперь {})", src.len());
        }
        let mut src = BytesMut::from(&wire[..]);
        let v = decode(&mut src).expect("без ошибки").expect("массив целый");
        assert_eq!(v, Value::Array(vec![b("ab"), Value::Array(vec![Value::Int(1), Value::Int(2)]), Value::Simple("tail".into())]), "третий элемент это статус tail");
        assert!(src.is_empty(), "буфер пуст");
    }

    #[test]
    fn stops_exactly_at_array_end() {
        let mut src = BytesMut::from(&b"*1\r\n:1\r\n:2\r\n"[..]);
        assert_eq!(decode(&mut src), Ok(Some(Value::Array(vec![Value::Int(1)]))), "в массиве один элемент");
        assert_eq!(&src[..], b":2\r\n", "следующий ответ не тронут");
    }
}
