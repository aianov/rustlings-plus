use std::error::Error;
use std::fmt;
use std::io;

use bytes::{BufMut, BytesMut};

use crate::codec::{Decoder, Encoder};

#[derive(Debug)]
pub enum LinesError {
    TooLong { max: usize },
    BadUtf8,
    Io(io::Error),
}

impl From<io::Error> for LinesError {
    fn from(error: io::Error) -> LinesError {
        LinesError::Io(error)
    }
}

impl fmt::Display for LinesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinesError::TooLong { max } => write!(f, "строка длиннее {max} байт"),
            LinesError::BadUtf8 => write!(f, "строка не в UTF-8"),
            LinesError::Io(..) => write!(f, "соединение оборвалось"),
        }
    }
}

impl Error for LinesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LinesError::Io(error) => Some(error),
            LinesError::TooLong { .. } | LinesError::BadUtf8 => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinesCodec {
    max_len: usize,
}

impl LinesCodec {
    pub fn new(max_len: usize) -> LinesCodec {
        LinesCodec { max_len }
    }

    pub fn max_len(&self) -> usize {
        self.max_len
    }
}

impl Decoder for LinesCodec {
    type Item = String;
    type Error = LinesError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<String>, LinesError> {
        let max = self.max_len;
        let Some(end) = src.iter().position(|byte| *byte == b'\n') else {
            if src.len() > max + 1 {
                return Err(LinesError::TooLong { max });
            }
            return Ok(None);
        };
        let mut line = src.split_to(end + 1);
        line.truncate(end);
        if line.last() == Some(&b'\r') {
            line.truncate(end - 1);
        }
        if line.len() > max {
            return Err(LinesError::TooLong { max });
        }
        String::from_utf8(line.to_vec())
            .map(Some)
            .map_err(|_| LinesError::BadUtf8)
    }
}

impl Encoder<String> for LinesCodec {
    type Error = LinesError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), LinesError> {
        Encoder::<&str>::encode(self, item.as_str(), dst)
    }
}

impl Encoder<&str> for LinesCodec {
    type Error = LinesError;

    fn encode(&mut self, item: &str, dst: &mut BytesMut) -> Result<(), LinesError> {
        if item.len() > self.max_len {
            return Err(LinesError::TooLong { max: self.max_len });
        }
        dst.reserve(item.len() + 1);
        dst.put_slice(item.as_bytes());
        dst.put_u8(b'\n');
        Ok(())
    }
}
