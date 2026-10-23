use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::codec::{Decoder, Encoder};

pub struct Framed<S, C> {
    stream: S,
    codec: C,
    read_buf: BytesMut,
    write_buf: BytesMut,
    eof: bool,
}

impl<S, C> Framed<S, C> {
    pub fn new(stream: S, codec: C) -> Framed<S, C> {
        Framed {
            stream,
            codec,
            read_buf: BytesMut::new(),
            write_buf: BytesMut::new(),
            eof: false,
        }
    }

    pub fn get_ref(&self) -> &S {
        &self.stream
    }

    pub fn codec(&self) -> &C {
        &self.codec
    }

    pub fn buffered(&self) -> usize {
        self.read_buf.len()
    }

    pub fn into_parts(self) -> (S, C, BytesMut) {
        (self.stream, self.codec, self.read_buf)
    }
}

impl<S: AsyncRead + Unpin, C: Decoder> Framed<S, C> {
    pub async fn recv(&mut self) -> Result<Option<C::Item>, C::Error> {
        loop {
            if self.eof {
                return self.codec.decode_eof(&mut self.read_buf);
            }
            if let Some(item) = self.codec.decode(&mut self.read_buf)? {
                return Ok(Some(item));
            }
            if self.stream.read_buf(&mut self.read_buf).await? == 0 {
                self.eof = true;
            }
        }
    }
}

impl<S: AsyncWrite + Unpin, C> Framed<S, C> {
    pub async fn send<I>(&mut self, item: I) -> Result<(), C::Error>
    where
        C: Encoder<I>,
    {
        self.write_buf.clear();
        self.codec.encode(item, &mut self.write_buf)?;
        self.stream.write_all(&self.write_buf).await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush().await
    }

    pub async fn close(&mut self) -> std::io::Result<()> {
        self.stream.shutdown().await
    }
}
