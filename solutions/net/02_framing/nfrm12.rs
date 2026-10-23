use bytes::{Buf, BufMut, BytesMut};
use std::io;

trait Decoder {
    type Item;
    type Error: From<io::Error>;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.decode(src)? {
            Some(item) => Ok(Some(item)),
            None if src.is_empty() => Ok(None),
            None => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "поток кончился посреди сообщения").into()),
        }
    }
}

trait Encoder<Item> {
    type Error: From<io::Error>;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error>;
}

#[derive(Debug)]
enum LinesError {
    TooLong,
    BadUtf8,
    Io(io::Error),
}

impl From<io::Error> for LinesError {
    fn from(e: io::Error) -> Self {
        LinesError::Io(e)
    }
}

struct LinesCodec {
    max_len: usize,
}

impl Decoder for LinesCodec {
    type Item = String;
    type Error = LinesError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<String>, LinesError> {
        let Some(end) = src.iter().position(|b| *b == b'\n') else {
            return if src.len() > self.max_len + 1 { Err(LinesError::TooLong) } else { Ok(None) };
        };
        let mut line = src.split_to(end + 1);
        line.truncate(end);
        if line.last() == Some(&b'\r') {
            line.truncate(end - 1);
        }
        if line.len() > self.max_len {
            return Err(LinesError::TooLong);
        }
        String::from_utf8(line.to_vec()).map(Some).map_err(|_| LinesError::BadUtf8)
    }
}

impl Encoder<String> for LinesCodec {
    type Error = LinesError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), LinesError> {
        if item.len() > self.max_len {
            return Err(LinesError::TooLong);
        }
        dst.reserve(item.len() + 1);
        dst.put_slice(item.as_bytes());
        dst.put_u8(b'\n');
        Ok(())
    }
}

fn main() {
    let mut codec = LinesCodec { max_len: 64 };
    let mut buf = BytesMut::new();
    codec.encode("привет".to_owned(), &mut buf).unwrap();
    println!("{:?} {:?}", codec.decode(&mut buf), buf.remaining());
    buf.put_u8(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec(max: usize) -> LinesCodec {
        LinesCodec { max_len: max }
    }

    #[test]
    fn decodes_lines_one_by_one() {
        let mut c = codec(16);
        let mut buf = BytesMut::from(&b"ab\ncd\r\ne"[..]);
        assert_eq!(c.decode(&mut buf).unwrap().as_deref(), Some("ab"));
        assert_eq!(c.decode(&mut buf).unwrap().as_deref(), Some("cd"), "'\\r' перед '\\n' не часть строки");
        assert!(c.decode(&mut buf).unwrap().is_none(), "третья строка не закончена");
        assert_eq!(&buf[..], b"e", "хвост должен остаться");
    }

    #[test]
    fn every_split_point() {
        let stream = b"one\ntwo\r\nthree\n";
        for k in 0..=stream.len() {
            let mut c = codec(16);
            let mut buf = BytesMut::new();
            let mut got = Vec::new();
            for part in [&stream[..k], &stream[k..]] {
                buf.extend_from_slice(part);
                while let Some(line) = c.decode(&mut buf).unwrap() {
                    got.push(line);
                }
            }
            assert_eq!(got, ["one", "two", "three"], "разрез на позиции {k} дал другой результат");
        }
    }

    #[test]
    fn limit_works_with_and_without_newline() {
        let mut c = codec(4);
        let mut ok = BytesMut::from(&b"abcd\n"[..]);
        assert_eq!(c.decode(&mut ok).unwrap().as_deref(), Some("abcd"), "ровно max_len проходит");
        let mut long = BytesMut::from(&b"abcde\n"[..]);
        assert!(matches!(c.decode(&mut long), Err(LinesError::TooLong)), "5 байт при лимите 4 это TooLong");
        let mut endless = BytesMut::from(&b"abcdefghij"[..]);
        assert!(
            matches!(c.decode(&mut endless), Err(LinesError::TooLong)),
            "'\\n' так и не пришёл, а накоплено уже больше лимита: надо отказывать сразу, а не ждать"
        );
        let mut short = BytesMut::from(&b"abc"[..]);
        assert!(matches!(c.decode(&mut short), Ok(None)), "пока меньше лимита, ждём дальше");
    }

    #[test]
    fn bad_utf8() {
        let mut c = codec(8);
        let mut buf = BytesMut::from(&b"\xff\xfe\n"[..]);
        assert!(matches!(c.decode(&mut buf), Err(LinesError::BadUtf8)), "не UTF-8");
    }

    #[test]
    fn eof_rules_come_from_the_trait() {
        let mut c = codec(8);
        let mut clean = BytesMut::new();
        assert!(c.decode_eof(&mut clean).unwrap().is_none(), "пустой буфер при EOF штатно");
        let mut cut = BytesMut::from(&b"abc"[..]);
        match c.decode_eof(&mut cut) {
            Err(LinesError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof),
            other => panic!("ждали Io(UnexpectedEof), получили {other:?}"),
        }
    }

    #[test]
    fn encoder_appends_and_checks_the_limit() {
        let mut c = codec(5);
        let mut buf = BytesMut::new();
        c.encode("hi".into(), &mut buf).unwrap();
        c.encode("there".into(), &mut buf).unwrap();
        assert_eq!(&buf[..], b"hi\nthere\n", "строки дописываются в конец, каждая с '\\n'");
        assert!(matches!(c.encode("toolong".into(), &mut buf), Err(LinesError::TooLong)), "7 байт при лимите 5");
        assert_eq!(buf.len(), 9, "при отказе в буфер ничего дописываться не должно");
    }

    #[test]
    fn cr_arrives_before_newline() {
        let mut c = codec(3);
        let mut buf = BytesMut::from(&b"abc\r"[..]);
        assert!(matches!(c.decode(&mut buf), Ok(None)), "'abc\\r' при лимите 3 это ещё не ошибка: '\\r' может оказаться частью '\\r\\n'");
        buf.extend_from_slice(b"\n");
        assert_eq!(c.decode(&mut buf).unwrap().as_deref(), Some("abc"));
    }
}
