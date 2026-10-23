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
        todo!()
    }
}

impl Encoder<Vec<u8>> for LengthCodec {
    type Error = LengthError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), LengthError> {
        todo!()
    }
}
