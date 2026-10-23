use bytes::{Buf, BytesMut};

use crate::Value;

#[derive(Debug, Clone)]
pub(crate) struct Limits {
    pub max_bulk: usize,
    pub max_array: usize,
    pub max_depth: usize,
}

pub(crate) fn encode_command<S: AsRef<[u8]>>(args: &[S]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for arg in args {
        let arg = arg.as_ref();
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}

pub(crate) fn decode(src: &mut BytesMut, limits: &Limits) -> Result<Option<Value>, String> {
    match parse_at(src, 0, 0, limits)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

fn parse_at(buf: &[u8], pos: usize, depth: usize, limits: &Limits) -> Result<Option<(Value, usize)>, String> {
    let Some(rel) = buf.get(pos..).and_then(|rest| rest.windows(2).position(|w| w == b"\r\n")) else {
        return Ok(None);
    };
    let end = pos + rel;
    let after = end + 2;
    let kind = buf[pos];
    if !matches!(kind, b'+' | b'-' | b':' | b'$' | b'*') {
        return Err(format!("неизвестный тип ответа {kind:#x}"));
    }
    let line = std::str::from_utf8(&buf[pos + 1..end]).map_err(|_| "строка ответа не UTF-8".to_string())?;
    let number = |text: &str| text.parse::<i64>().map_err(|_| format!("не число: {text:?}"));
    match kind {
        b'+' => Ok(Some((Value::Simple(line.to_string()), after))),
        b'-' => Ok(Some((Value::Error(line.to_string()), after))),
        b':' => Ok(Some((Value::Int(number(line)?), after))),
        b'$' => {
            let len = number(line)?;
            if len == -1 {
                return Ok(Some((Value::Null, after)));
            }
            let len = usize::try_from(len).map_err(|_| format!("отрицательная длина {len}"))?;
            if len > limits.max_bulk {
                return Err(format!("значение {len} байтов больше предела {}", limits.max_bulk));
            }
            let body_end = after + len;
            for (i, expected) in [b'\r', b'\n'].iter().enumerate() {
                match buf.get(body_end + i) {
                    None => return Ok(None),
                    Some(b) if b == expected => {}
                    Some(_) => return Err("после значения нет CRLF".to_string()),
                }
            }
            Ok(Some((Value::Bulk(buf[after..body_end].to_vec()), body_end + 2)))
        }
        _ => {
            let n = number(line)?;
            if n == -1 {
                return Ok(Some((Value::Null, after)));
            }
            let n = usize::try_from(n).map_err(|_| format!("отрицательное число элементов {n}"))?;
            if n > limits.max_array {
                return Err(format!("массив из {n} элементов больше предела {}", limits.max_array));
            }
            if depth >= limits.max_depth {
                return Err("массивы вложены слишком глубоко".to_string());
            }
            let mut items = Vec::new();
            let mut next = after;
            for _ in 0..n {
                let Some((item, item_end)) = parse_at(buf, next, depth + 1, limits)? else {
                    return Ok(None);
                };
                items.push(item);
                next = item_end;
            }
            Ok(Some((Value::Array(items), next)))
        }
    }
}
