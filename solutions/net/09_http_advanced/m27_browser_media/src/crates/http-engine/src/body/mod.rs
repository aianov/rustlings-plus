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
    pub fn new(
        reader: R,
        buf: Vec<u8>,
        framing: Framing,
        limit: usize,
    ) -> io::Result<BodyReader<R>> {
        let state = match framing {
            Framing::None | Framing::Length(0) => State::Done,
            Framing::Length(n) if n > limit as u64 => return invalid("тело больше лимита"),
            Framing::Length(n) => State::Length(n),
            Framing::Chunked => State::ChunkSize,
        };
        Ok(BodyReader {
            reader,
            buf,
            state,
            limit,
            seen: 0,
        })
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
        let n = self
            .buf
            .len()
            .min(usize::try_from(left).unwrap_or(usize::MAX))
            .min(4096);
        Ok(self.buf.drain(..n).collect())
    }

    pub async fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            match self.state {
                State::Done => return Ok(None),
                State::Length(left) => {
                    let piece = self.piece(left).await?;
                    let left = left - piece.len() as u64;
                    self.state = if left == 0 {
                        State::Done
                    } else {
                        State::Length(left)
                    };
                    return Ok(Some(piece));
                }
                State::ChunkSize => {
                    let line = self.line().await?;
                    let size_text = line.split(';').next().unwrap_or_default().trim();
                    if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return invalid("плохой размер куска");
                    }
                    let size = u64::from_str_radix(size_text, 16)
                        .or_else(|_| invalid("размер куска слишком велик"))?;
                    if size == 0 {
                        self.state = State::Trailers;
                    } else if size > (self.limit - self.seen) as u64 {
                        return invalid("тело больше лимита");
                    } else {
                        self.state = State::ChunkData(size);
                    }
                }
                State::ChunkData(left) => {
                    let piece = self.piece(left).await?;
                    self.seen += piece.len();
                    let left = left - piece.len() as u64;
                    self.state = if left == 0 {
                        State::ChunkEnd
                    } else {
                        State::ChunkData(left)
                    };
                    return Ok(Some(piece));
                }
                State::ChunkEnd => {
                    if !self.line().await?.is_empty() {
                        return invalid("после куска нет CRLF");
                    }
                    self.state = State::ChunkSize;
                }
                State::Trailers => {
                    if self.line().await?.is_empty() {
                        self.state = State::Done;
                    }
                }
            }
        }
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
