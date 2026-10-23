use crate::codec::{Decoder, Encoder};
use bytes::{BufMut, BytesMut};
use std::io;

#[derive(Debug)]
pub enum LinesError {
    TooLong,
    BadUtf8,
    Io(io::Error),
}

impl From<io::Error> for LinesError {
    fn from(e: io::Error) -> Self {
        LinesError::Io(e)
    }
}

pub struct LinesCodec {
    max_len: usize,
}

impl LinesCodec {
    pub fn new(max_len: usize) -> LinesCodec {
        LinesCodec { max_len }
    }
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
