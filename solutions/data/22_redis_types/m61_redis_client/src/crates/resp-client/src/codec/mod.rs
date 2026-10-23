use std::fmt;
use std::io;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use framing::{Decoder, Encoder};

use crate::command::Cmd;
use crate::value::Value;

#[derive(Debug)]
pub enum RespError {
    TooLarge {
        what: &'static str,
        len: usize,
        max: usize,
    },
    TooDeep(usize),
    BadFrame(String),
    Server(String),
    Timeout,
    Closed,
    Io(io::Error),
}

impl From<io::Error> for RespError {
    fn from(error: io::Error) -> RespError {
        RespError::Io(error)
    }
}

impl fmt::Display for RespError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RespError::TooLarge { what, len, max } => {
                write!(f, "{what} на {len}, а можно не больше {max}")
            }
            RespError::TooDeep(depth) => write!(f, "вложенность ответа глубже {depth}"),
            RespError::BadFrame(why) => write!(f, "кривой кадр RESP: {why}"),
            RespError::Server(why) => write!(f, "Redis ответил ошибкой: {why}"),
            RespError::Timeout => write!(f, "Redis не ответил в срок"),
            RespError::Closed => write!(f, "соединение с Redis закрыто"),
            RespError::Io(error) => write!(f, "сокет Redis: {error}"),
        }
    }
}

impl std::error::Error for RespError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RespCodec {
    pub max_bulk: usize,
    pub max_items: usize,
    pub max_depth: usize,
}

impl Default for RespCodec {
    fn default() -> RespCodec {
        RespCodec {
            max_bulk: 16 * 1024 * 1024,
            max_items: 1024 * 1024,
            max_depth: 8,
        }
    }
}

fn line(src: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let rest = src.get(at..)?;
    let end = rest.windows(2).position(|pair| pair == b"\r\n")?;
    Some((&rest[..end], at + end + 2))
}

fn number(raw: &[u8]) -> Result<i64, RespError> {
    std::str::from_utf8(raw)
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| RespError::BadFrame(String::from("число")))
}

impl RespCodec {
    fn parse(
        &self,
        src: &[u8],
        at: usize,
        depth: usize,
    ) -> Result<Option<(Value, usize)>, RespError> {
        if depth > self.max_depth {
            return Err(RespError::TooDeep(self.max_depth));
        }
        let Some(&kind) = src.get(at) else {
            return Ok(None);
        };
        let Some((head, next)) = line(src, at + 1) else {
            return Ok(None);
        };
        match kind {
            b'+' => Ok(Some((
                Value::Simple(String::from_utf8_lossy(head).into_owned()),
                next,
            ))),
            b'-' => Ok(Some((
                Value::Error(String::from_utf8_lossy(head).into_owned()),
                next,
            ))),
            b':' => Ok(Some((Value::Integer(number(head)?), next))),
            b'$' => {
                let len = number(head)?;
                if len < 0 {
                    return Ok(Some((Value::Bulk(None), next)));
                }
                let len = usize::try_from(len).unwrap_or(usize::MAX);
                if len > self.max_bulk {
                    return Err(RespError::TooLarge {
                        what: "строка",
                        len,
                        max: self.max_bulk,
                    });
                }
                if src.len() < next + len + 2 {
                    return Ok(None);
                }
                if &src[next + len..next + len + 2] != b"\r\n" {
                    return Err(RespError::BadFrame(String::from("строка без \\r\\n")));
                }
                let bytes = Bytes::copy_from_slice(&src[next..next + len]);
                Ok(Some((Value::Bulk(Some(bytes)), next + len + 2)))
            }
            b'*' => {
                let count = number(head)?;
                if count < 0 {
                    return Ok(Some((Value::Array(None), next)));
                }
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                if count > self.max_items {
                    return Err(RespError::TooLarge {
                        what: "массив",
                        len: count,
                        max: self.max_items,
                    });
                }
                let mut items = Vec::with_capacity(count.min(1024));
                let mut cursor = next;
                for _ in 0..count {
                    match self.parse(src, cursor, depth + 1)? {
                        Some((item, after)) => {
                            items.push(item);
                            cursor = after;
                        }
                        None => return Ok(None),
                    }
                }
                Ok(Some((Value::Array(Some(items)), cursor)))
            }
            other => Err(RespError::BadFrame(format!(
                "неизвестный тип {:?}",
                other as char
            ))),
        }
    }
}

impl Decoder for RespCodec {
    type Item = Value;
    type Error = RespError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Value>, RespError> {
        match self.parse(src, 0, 0)? {
            Some((value, used)) => {
                src.advance(used);
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

impl Encoder<&Cmd> for RespCodec {
    type Error = RespError;

    fn encode(&mut self, command: &Cmd, dst: &mut BytesMut) -> Result<(), RespError> {
        let parts = command.parts();
        dst.put_slice(format!("*{}\r\n", parts.len()).as_bytes());
        for part in parts {
            dst.put_slice(format!("${}\r\n", part.len()).as_bytes());
            dst.put_slice(part);
            dst.put_slice(b"\r\n");
        }
        Ok(())
    }
}
