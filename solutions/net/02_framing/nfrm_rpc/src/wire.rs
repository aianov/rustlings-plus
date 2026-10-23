use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_FRAME: usize = 16 * 1024 * 1024;

pub struct Frame {
    pub id: u64,
    pub body: Vec<u8>,
}

pub fn encode(frame: &Frame) -> Bytes {
    let mut out = BytesMut::with_capacity(12 + frame.body.len());
    out.put_u32((8 + frame.body.len()) as u32);
    out.put_u64(frame.id);
    out.put_slice(&frame.body);
    out.freeze()
}

pub struct FrameReader<R> {
    reader: R,
    buf: BytesMut,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> FrameReader<R> {
        FrameReader { reader, buf: BytesMut::new() }
    }

    pub async fn next(&mut self) -> io::Result<Option<Frame>> {
        loop {
            if self.buf.len() >= 4 {
                let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
                if !(8..=MAX_FRAME).contains(&len) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "кадр недопустимой длины"));
                }
                if self.buf.len() >= 4 + len {
                    self.buf.advance(4);
                    let id = self.buf.get_u64();
                    let body = self.buf.split_to(len - 8).to_vec();
                    return Ok(Some(Frame { id, body }));
                }
            }
            if self.reader.read_buf(&mut self.buf).await? == 0 {
                return if self.buf.is_empty() { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
            }
        }
    }
}
