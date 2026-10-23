// Собираем всё вместе в один кодек, каким его будет использовать клиент Redis: команды уходят массивами
// bulk-строк, ответы приходят потоком кусков, как получится, и разбираются с защитой от плохих данных.
// Тесты подают кодеку поток по одному байту и с разрезами в случайных местах, а ещё сравнивают
// кодирование и разбор на случайных деревьях значений.
// TODO: Напиши кодек. `encode` дописывает значение в конец буфера (статус и ошибка с переводом
// строки это BadText, и при ошибке в буфер не попадает ни байта). `encode_command` пишет команду.
// `decode` как в прошлых задачах, пределы по заголовку. `decode_eof` вызывают, когда соединение закрылось.

use bytes::{Buf, BytesMut};

#[derive(Debug, PartialEq, Clone)]
enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Value>),
    NullArray,
}

#[derive(Debug, Clone)]
struct Limits {
    max_bulk: usize,
    max_array: usize,
    max_depth: usize,
}

#[derive(Debug, PartialEq)]
enum CodecError {
    BadType(u8),
    BadNumber,
    BadLength,
    MissingCrlf,
    BadUtf8,
    TooLarge,
    TooDeep,
    Truncated,
    BadText,
}

struct RespCodec {
    limits: Limits,
}

impl RespCodec {
    fn new(limits: Limits) -> RespCodec {
        todo!()
    }

    fn encode(&mut self, value: &Value, dst: &mut BytesMut) -> Result<(), CodecError> {
        todo!()
    }

    fn encode_command(&mut self, args: &[&[u8]], dst: &mut BytesMut) {
        todo!()
    }

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Value>, CodecError> {
        todo!()
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Value>, CodecError> {
        todo!()
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

    fn limits() -> Limits {
        Limits { max_bulk: 1024, max_array: 64, max_depth: 8 }
    }

    fn word(rng: &mut Lcg) -> String {
        let alphabet = b"abc xyz-+:$*";
        (0..rng.next() % 12).map(|_| alphabet[rng.next() % alphabet.len()] as char).collect()
    }

    fn blob(rng: &mut Lcg) -> Vec<u8> {
        (0..rng.next() % 30)
            .map(|_| match rng.next() % 4 {
                0 => b'\r',
                1 => b'\n',
                _ => (rng.next() % 256) as u8,
            })
            .collect()
    }

    fn random_value(rng: &mut Lcg, depth: usize) -> Value {
        let kinds = if depth >= 3 { 6 } else { 7 };
        match rng.next() % kinds {
            0 => Value::Simple(word(rng)),
            1 => Value::Error(format!("ERR {}", word(rng))),
            2 => Value::Int(rng.next() as i64 - 500_000),
            3 => Value::Bulk(blob(rng)),
            4 => Value::Null,
            5 => Value::NullArray,
            _ => Value::Array((0..rng.next() % 5).map(|_| random_value(rng, depth + 1)).collect()),
        }
    }

    fn decode_chunked(codec: &mut RespCodec, wire: &[u8], rng: &mut Lcg, max_chunk: usize) -> Vec<Value> {
        let mut src = BytesMut::new();
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < wire.len() {
            let n = (1 + rng.next() % max_chunk).min(wire.len() - pos);
            src.extend_from_slice(&wire[pos..pos + n]);
            pos += n;
            while let Some(v) = codec.decode(&mut src).expect("корректные данные не должны давать ошибку") {
                out.push(v);
            }
        }
        assert!(src.is_empty(), "после последнего кадра в буфере остались байты: {}", src.len());
        out
    }

    #[test]
    fn encodes_every_kind() {
        let mut codec = RespCodec::new(limits());
        let mut dst = BytesMut::new();
        let value = Value::Array(vec![
            Value::Simple("OK".into()),
            Value::Error("ERR bad".into()),
            Value::Int(-3),
            Value::Bulk(b"a\r\nb".to_vec()),
            Value::Null,
            Value::NullArray,
            Value::Array(vec![]),
        ]);
        codec.encode(&value, &mut dst).expect("все виды кодируются");
        assert_eq!(&dst[..], b"*7\r\n+OK\r\n-ERR bad\r\n:-3\r\n$4\r\na\r\nb\r\n$-1\r\n*-1\r\n*0\r\n", "байты массива из всех видов");
    }

    #[test]
    fn encode_appends_and_keeps_old_bytes() {
        let mut codec = RespCodec::new(limits());
        let mut dst = BytesMut::from(&b"keep"[..]);
        codec.encode(&Value::Int(1), &mut dst).expect("кодируется");
        codec.encode(&Value::Int(2), &mut dst).expect("кодируется");
        assert_eq!(&dst[..], b"keep:1\r\n:2\r\n", "кодек дописывает в конец и не затирает то, что уже в буфере");
    }

    #[test]
    fn line_break_in_status_is_refused_without_partial_output() {
        let mut codec = RespCodec::new(limits());
        let mut dst = BytesMut::from(&b"keep"[..]);
        let bad = Value::Array(vec![Value::Int(1), Value::Simple("a\nb".into())]);
        assert_eq!(codec.encode(&bad, &mut dst), Err(CodecError::BadText), "перевод строки в статусе");
        assert_eq!(&dst[..], b"keep", "при ошибке в буфер не должно попасть ни байта, даже от элементов до плохого");
        assert_eq!(codec.encode(&Value::Error("x\r".into()), &mut dst), Err(CodecError::BadText), "и в ошибке тоже");
    }

    #[test]
    fn command_is_array_of_bulks() {
        let mut codec = RespCodec::new(limits());
        let mut dst = BytesMut::new();
        codec.encode_command(&[b"SET", b"k", b"v\r\nw"], &mut dst);
        assert_eq!(&dst[..], b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$4\r\nv\r\nw\r\n", "команда SET со значением из двух строк");
    }

    #[test]
    fn random_values_survive_a_round_trip_with_random_cuts() {
        let mut rng = Lcg(2024);
        let mut codec = RespCodec::new(limits());
        for round in 0..300 {
            let values: Vec<Value> = (0..1 + rng.next() % 4).map(|_| random_value(&mut rng, 0)).collect();
            let mut wire = BytesMut::new();
            for v in &values {
                codec.encode(v, &mut wire).expect("значения без переводов строки в статусах");
            }
            let got = decode_chunked(&mut codec, &wire, &mut rng, 7);
            assert_eq!(got, values, "повтор {round}: значения после кодирования и разбора с разрезами должны совпасть");
        }
    }

    #[test]
    fn long_pipeline_byte_by_byte() {
        let mut codec = RespCodec::new(limits());
        let mut wire = BytesMut::new();
        let mut expected = Vec::new();
        for i in 0..200 {
            let key = format!("user:{i}");
            let val = format!("v{i}\r\n{i}");
            codec.encode_command(&[b"SET", key.as_bytes(), val.as_bytes()], &mut wire);
            expected.push(Value::Array(vec![
                Value::Bulk(b"SET".to_vec()),
                Value::Bulk(key.into_bytes()),
                Value::Bulk(val.into_bytes()),
            ]));
        }
        let mut rng = Lcg(1);
        let got = decode_chunked(&mut codec, &wire, &mut rng, 1);
        assert_eq!(got, expected, "двести команд, поданных по одному байту");
    }

    #[test]
    fn big_bulk_in_chunks() {
        let mut codec = RespCodec::new(Limits { max_bulk: 1 << 20, max_array: 8, max_depth: 4 });
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut wire = BytesMut::new();
        codec.encode(&Value::Bulk(payload.clone()), &mut wire).expect("кодируется");
        let mut rng = Lcg(3);
        let got = decode_chunked(&mut codec, &wire, &mut rng, 8192);
        assert_eq!(got, vec![Value::Bulk(payload)], "триста килобайт кусками по несколько килобайт");
    }

    #[test]
    fn limits_are_checked_by_header() {
        let mut codec = RespCodec::new(Limits { max_bulk: 16, max_array: 8, max_depth: 3 });
        assert_eq!(codec.decode(&mut BytesMut::from(&b"$17\r\n"[..])), Err(CodecError::TooLarge), "bulk больше предела");
        assert_eq!(codec.decode(&mut BytesMut::from(&b"*9\r\n"[..])), Err(CodecError::TooLarge), "массив больше предела");
        assert_eq!(codec.decode(&mut BytesMut::from(&b"*1\r\n*1\r\n*1\r\n*1\r\n"[..])), Err(CodecError::TooDeep), "четвёртый уровень при пределе три");
        let deep = b"*1\r\n".repeat(100_000);
        assert_eq!(codec.decode(&mut BytesMut::from(&deep[..])), Err(CodecError::TooDeep), "стек не должен кончиться раньше, чем сработает предел");
    }

    #[test]
    fn protocol_errors() {
        let mut codec = RespCodec::new(limits());
        assert_eq!(codec.decode(&mut BytesMut::from(&b"!x\r\n"[..])), Err(CodecError::BadType(b'!')), "неизвестный тип");
        assert_eq!(codec.decode(&mut BytesMut::from(&b"$-7\r\n"[..])), Err(CodecError::BadLength), "отрицательная длина");
        assert_eq!(codec.decode(&mut BytesMut::from(&b":1x\r\n"[..])), Err(CodecError::BadNumber), "мусор в числе");
        assert_eq!(codec.decode(&mut BytesMut::from(&b"$2\r\nabcd"[..])), Err(CodecError::MissingCrlf), "тело длиннее объявленного");
    }

    #[test]
    fn end_of_stream() {
        let mut codec = RespCodec::new(limits());
        let mut clean = BytesMut::from(&b"+OK\r\n"[..]);
        assert_eq!(codec.decode_eof(&mut clean), Ok(Some(Value::Simple("OK".into()))), "последний кадр отдаётся и в конце потока");
        assert_eq!(codec.decode_eof(&mut clean), Ok(None), "пустой буфер в конце потока это нормальное закрытие");
        let mut cut = BytesMut::from(&b"$5\r\nhel"[..]);
        assert_eq!(codec.decode_eof(&mut cut), Err(CodecError::Truncated), "сервер закрыл соединение посреди ответа: это ошибка, а не молчание");
    }
}
