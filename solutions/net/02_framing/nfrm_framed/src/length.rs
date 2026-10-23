use crate::codec::{Decoder, Encoder};
use bytes::{Buf, BufMut, BytesMut};
use std::io;

#[derive(Debug)]
pub enum LengthError {
    TooLong,
    Io(io::Error),
}

impl From<io::Error> for LengthError {
    fn from(e: io::Error) -> Self {
        LengthError::Io(e)
    }
}

pub struct LengthCodec {
    max_len: usize,
}

impl LengthCodec {
    pub fn new(max_len: usize) -> LengthCodec {
        LengthCodec { max_len }
    }
}

impl Decoder for LengthCodec {
    type Item = Vec<u8>;
    type Error = LengthError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Vec<u8>>, LengthError> {
        if src.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;
        if len > self.max_len {
            return Err(LengthError::TooLong);
        }
        if src.len() < 4 + len {
            src.reserve(4 + len - src.len());
            return Ok(None);
        }
        src.advance(4);
        Ok(Some(src.split_to(len).to_vec()))
    }
}

impl Encoder<Vec<u8>> for LengthCodec {
    type Error = LengthError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), LengthError> {
        if item.len() > self.max_len {
            return Err(LengthError::TooLong);
        }
        dst.reserve(4 + item.len());
        dst.put_u32(item.len() as u32);
        dst.put_slice(&item);
        Ok(())
    }
}
