use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    None,
    Length(u64),
    Chunked,
}

enum State {
    Length(u64),
    ChunkSize,
    ChunkData(u64),
    ChunkEnd,
    Trailers,
    Done,
}

pub struct BodyReader<R> {
    reader: R,
    buf: Vec<u8>,
    state: State,
    limit: usize,
    seen: usize,
}

fn invalid<T>(why: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, why.to_string()))
}

impl<R: AsyncRead + Unpin> BodyReader<R> {
    pub fn new(reader: R, buf: Vec<u8>, framing: Framing, limit: usize) -> io::Result<BodyReader<R>> {
        let state = match framing {
            Framing::None | Framing::Length(0) => State::Done,
            Framing::Length(n) if n > limit as u64 => return invalid("тело больше лимита"),
            Framing::Length(n) => State::Length(n),
            Framing::Chunked => State::ChunkSize,
        };
        Ok(BodyReader { reader, buf, state, limit, seen: 0 })
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    pub fn into_parts(self) -> (R, Vec<u8>) {
        (self.reader, self.buf)
    }

    async fn more(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 4096];
        let n = self.reader.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    async fn line(&mut self) -> io::Result<String> {
        loop {
            if let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.buf.drain(..pos + 2).take(pos).collect();
                return String::from_utf8(line).or_else(|_| invalid("строка не в utf-8"));
            }
            if self.buf.len() > 4096 {
                return invalid("слишком длинная строка");
            }
            self.more().await?;
        }
    }

    async fn piece(&mut self, left: u64) -> io::Result<Vec<u8>> {
        if self.buf.is_empty() {
            self.more().await?;
        }
        let n = self.buf.len().min(left as usize).min(4096);
        Ok(self.buf.drain(..n).collect())
    }

    pub async fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        todo!()
    }

    pub async fn drain(&mut self, cap: usize) -> io::Result<bool> {
        let mut total = 0;
        while let Some(piece) = self.next_chunk().await? {
            total += piece.len();
            if total > cap {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
