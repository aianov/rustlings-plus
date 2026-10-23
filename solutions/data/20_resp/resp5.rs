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

fn parse(buf: &[u8]) -> Result<Option<(Value, usize)>, Bad> {
    let Some(end) = buf.windows(2).position(|w| w == b"\r\n") else {
        return Ok(None);
    };
    let after = end + 2;
    let line = std::str::from_utf8(buf.get(1..end).ok_or(Bad)?).map_err(|_| Bad)?;
    match buf[0] {
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
        _ => Err(Bad),
    }
}

fn decode(src: &mut BytesMut) -> Result<Option<Value>, Bad> {
    match parse(src)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as usize
        }
    }

    const STREAM: &[u8] = b"+OK\r\n:42\r\n$3\r\nfoo\r\n$-1\r\n-ERR x\r\n$0\r\n\r\n";

    fn expected() -> Vec<Value> {
        vec![
            Value::Simple("OK".into()),
            Value::Int(42),
            Value::Bulk(b"foo".to_vec()),
            Value::Null,
            Value::Error("ERR x".into()),
            Value::Bulk(Vec::new()),
        ]
    }

    fn drain(src: &mut BytesMut, out: &mut Vec<Value>) {
        while let Some(v) = decode(src).expect("корректные данные не должны давать ошибку") {
            out.push(v);
        }
    }

    #[test]
    fn whole_stream_in_one_chunk() {
        let mut src = BytesMut::from(STREAM);
        let mut out = Vec::new();
        drain(&mut src, &mut out);
        assert_eq!(out, expected(), "все шесть ответов, которые лежат в буфере");
        assert!(src.is_empty(), "после последнего кадра в буфере остались лишние байты: {}", src.len());
    }

    #[test]
    fn every_prefix_is_incomplete_and_untouched() {
        let frame = b"$5\r\nhello\r\n";
        for cut in 0..frame.len() {
            let mut src = BytesMut::from(&frame[..cut]);
            assert_eq!(decode(&mut src), Ok(None), "после {cut} байтов кадр ещё не готов");
            assert_eq!(src.len(), cut, "неполный кадр нельзя трогать: после {cut} байтов в буфере должно остаться {cut}, а стало {}", src.len());
        }
        let mut src = BytesMut::from(&frame[..]);
        assert_eq!(decode(&mut src), Ok(Some(Value::Bulk(b"hello".to_vec()))), "целый кадр");
        assert!(src.is_empty(), "кадр съеден целиком");
    }

    #[test]
    fn byte_by_byte() {
        let mut src = BytesMut::new();
        let mut out = Vec::new();
        for b in STREAM {
            src.extend_from_slice(&[*b]);
            drain(&mut src, &mut out);
        }
        assert_eq!(out, expected(), "по одному байту результат должен быть тем же");
        assert!(src.is_empty(), "буфер пуст");
    }

    #[test]
    fn random_cuts() {
        let mut rng = Lcg(7);
        for round in 0..200 {
            let mut src = BytesMut::new();
            let mut out = Vec::new();
            let mut pos = 0;
            while pos < STREAM.len() {
                let n = (1 + rng.next() % 9).min(STREAM.len() - pos);
                src.extend_from_slice(&STREAM[pos..pos + n]);
                pos += n;
                drain(&mut src, &mut out);
            }
            assert_eq!(out, expected(), "разрез в случайных местах, повтор {round}");
        }
    }

    #[test]
    fn leftover_stays_in_buffer() {
        let mut src = BytesMut::from(&b"+OK\r\n:4"[..]);
        assert_eq!(decode(&mut src), Ok(Some(Value::Simple("OK".into()))), "первый кадр готов");
        assert_eq!(&src[..], b":4", "начало следующего кадра должно остаться в буфере как есть");
        assert_eq!(decode(&mut src), Ok(None), "второй кадр ещё не дописан");
        src.extend_from_slice(b"2\r\n");
        assert_eq!(decode(&mut src), Ok(Some(Value::Int(42))), "дописали: число 42");
    }

    #[test]
    fn unknown_type_is_error() {
        let mut src = BytesMut::from(&b"!oops\r\n"[..]);
        assert_eq!(decode(&mut src), Err(Bad), "неизвестный тип это ошибка, а не ожидание");
    }
}
