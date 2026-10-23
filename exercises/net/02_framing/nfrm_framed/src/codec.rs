use bytes::BytesMut;
use std::io;

pub trait Decoder {
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

pub trait Encoder<Item> {
    type Error: From<io::Error>;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error>;
}
