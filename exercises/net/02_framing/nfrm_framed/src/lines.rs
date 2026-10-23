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
        todo!()
    }
}

impl Encoder<String> for LinesCodec {
    type Error = LinesError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), LinesError> {
        todo!()
    }
}
