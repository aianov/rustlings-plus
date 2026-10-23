// Данные из сокета приходят как получится: ответ может прийти кусками, а в один кусок может влезть
// несколько ответов. Парсер, который видит «$5\r\nhel», не должен ни паниковать, ни терять эти
// байты, ни выдавать пол-ответа. Нужен разбор прямо из буфера, который растёт.
// TODO: Напиши `decode`: если кадр целиком в буфере, верни значение и убери из буфера ровно его байты;
// если кадр пока неполный, верни Ok(None) и оставь буфер нетронутым. Неизвестный тип это Err(Bad).

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

fn decode(src: &mut BytesMut) -> Result<Option<Value>, Bad> {
    todo!()
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
