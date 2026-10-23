use std::error::Error;
use std::fmt;
use std::io;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::codec::{Decoder, Encoder};

const HEADER: usize = 4;

#[derive(Debug)]
pub enum LengthError {
    TooLong { len: usize, max: usize },
    Io(io::Error),
}

impl From<io::Error> for LengthError {
    fn from(error: io::Error) -> LengthError {
        LengthError::Io(error)
    }
}

impl fmt::Display for LengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LengthError::TooLong { len, max } => {
                write!(f, "кадр на {len} байт, а можно не больше {max}")
            }
            LengthError::Io(..) => write!(f, "соединение оборвалось"),
        }
    }
}

impl Error for LengthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LengthError::Io(error) => Some(error),
            LengthError::TooLong { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LengthCodec {
    max_len: usize,
}

impl LengthCodec {
    pub fn new(max_len: usize) -> LengthCodec {
        LengthCodec { max_len }
    }

    pub fn max_len(&self) -> usize {
        self.max_len
    }
}

impl Decoder for LengthCodec {
    type Item = Bytes;
    type Error = LengthError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Bytes>, LengthError> {
        let Some(header) = src.get(..HEADER) else {
            return Ok(None);
        };
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if len > self.max_len {
            return Err(LengthError::TooLong {
                len,
                max: self.max_len,
            });
        }
        if src.len() < HEADER + len {
            src.reserve(HEADER + len - src.len());
            return Ok(None);
        }
        src.advance(HEADER);
        Ok(Some(src.split_to(len).freeze()))
    }
}

impl Encoder<&[u8]> for LengthCodec {
    type Error = LengthError;

    fn encode(&mut self, item: &[u8], dst: &mut BytesMut) -> Result<(), LengthError> {
        let too_long = LengthError::TooLong {
            len: item.len(),
            max: self.max_len,
        };
        if item.len() > self.max_len {
            return Err(too_long);
        }
        let len = u32::try_from(item.len()).map_err(|_| too_long)?;
        dst.reserve(HEADER + item.len());
        dst.put_u32(len);
        dst.put_slice(item);
        Ok(())
    }
}

impl Encoder<Bytes> for LengthCodec {
    type Error = LengthError;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), LengthError> {
        Encoder::<&[u8]>::encode(self, item.as_ref(), dst)
    }
}

impl Encoder<Vec<u8>> for LengthCodec {
    type Error = LengthError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), LengthError> {
        Encoder::<&[u8]>::encode(self, item.as_slice(), dst)
    }
}
