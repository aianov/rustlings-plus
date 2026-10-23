#![allow(dead_code)]
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
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

struct BodyReader<R> {
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
    fn new(reader: R, buf: Vec<u8>, framing: Framing, limit: usize) -> io::Result<BodyReader<R>> {
        let state = match framing {
            Framing::None | Framing::Length(0) => State::Done,
            Framing::Length(n) if n > limit as u64 => return invalid("тело больше лимита"),
            Framing::Length(n) => State::Length(n),
            Framing::Chunked => State::ChunkSize,
        };
        Ok(BodyReader { reader, buf, state, limit, seen: 0 })
    }

    fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn into_parts(self) -> (R, Vec<u8>) {
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

    async fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            match self.state {
                State::Done => return Ok(None),
                State::Length(left) => {
                    let piece = self.piece(left).await?;
                    let left = left - piece.len() as u64;
                    self.state = if left == 0 { State::Done } else { State::Length(left) };
                    return Ok(Some(piece));
                }
                State::ChunkSize => {
                    let line = self.line().await?;
                    let size_text = line.split(';').next().unwrap_or("").trim();
                    if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return invalid("плохой размер куска");
                    }
                    let size = u64::from_str_radix(size_text, 16).or_else(|_| invalid("размер куска слишком велик"))?;
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
                    self.state = if left == 0 { State::ChunkEnd } else { State::ChunkData(left) };
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

    async fn drain(&mut self, cap: usize) -> io::Result<bool> {
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Trickle {
        data: Vec<u8>,
        pos: usize,
        step: usize,
        reads: Arc<AtomicUsize>,
    }

    impl Trickle {
        fn new(data: &[u8], step: usize) -> (Trickle, Arc<AtomicUsize>) {
            let reads = Arc::new(AtomicUsize::new(0));
            (Trickle { data: data.to_vec(), pos: 0, step, reads: reads.clone() }, reads)
        }
    }

    impl AsyncRead for Trickle {
        fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, out: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let n = self.step.min(out.remaining()).min(self.data.len() - self.pos);
            let end = self.pos + n;
            out.put_slice(&self.data[self.pos..end]);
            self.pos = end;
            self.reads.fetch_add(n, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        }
    }

    struct Endless {
        taken: Arc<AtomicUsize>,
    }

    impl AsyncRead for Endless {
        fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, out: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let n = out.remaining().min(1024);
            out.put_slice(&vec![b'a'; n]);
            self.taken.fetch_add(n, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        }
    }

    async fn collect<R: AsyncRead + Unpin>(body: &mut BodyReader<R>) -> io::Result<(Vec<u8>, usize)> {
        let mut all = Vec::new();
        let mut biggest = 0;
        let work = async {
            while let Some(piece) = body.next_chunk().await? {
                biggest = biggest.max(piece.len());
                all.extend_from_slice(&piece);
            }
            Ok::<_, io::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(5), work).await.expect("чтение тела зависло")?;
        Ok((all, biggest))
    }

    #[tokio::test]
    async fn length_body_comes_in_small_pieces() {
        let mut wire = vec![b'x'; 10_000];
        wire.extend_from_slice(b"NEXT");
        let (reader, _) = Trickle::new(&wire, 1000);
        let mut body = BodyReader::new(reader, Vec::new(), Framing::Length(10_000), 1 << 20).unwrap();
        let (all, biggest) = collect(&mut body).await.unwrap();
        assert_eq!(all.len(), 10_000);
        assert!(biggest <= 4096, "тело отдаётся кусками не больше 4 КиБ, а не целиком: кусок был {biggest}");
        assert!(body.is_done());
        assert!(body.next_chunk().await.unwrap().is_none(), "после конца всегда None");
        let (mut reader, buf) = body.into_parts();
        let mut rest = buf;
        let mut tail = [0u8; 16];
        let n = reader.read(&mut tail).await.unwrap();
        rest.extend_from_slice(&tail[..n]);
        assert_eq!(rest, b"NEXT", "чужие байты после тела остаются нетронутыми (в буфере или в потоке)");
    }

    #[tokio::test]
    async fn bytes_already_buffered_come_first() {
        let (reader, _) = Trickle::new(b"cdefXYZ", 2);
        let mut body = BodyReader::new(reader, b"ab".to_vec(), Framing::Length(6), 100).unwrap();
        let (all, _) = collect(&mut body).await.unwrap();
        assert_eq!(all, b"abcdef");
        let (_, rest) = body.into_parts();
        assert!(rest.len() <= 1000 && !rest.contains(&b'f'), "в остатке только лишнее, не тело: {rest:?}");
    }

    #[tokio::test]
    async fn extra_bytes_in_the_buffer_are_not_the_body() {
        let (reader, reads) = Trickle::new(b"", 1);
        let mut body = BodyReader::new(reader, b"abcdefNEXT".to_vec(), Framing::Length(6), 100).unwrap();
        let (all, _) = collect(&mut body).await.unwrap();
        assert_eq!(all, b"abcdef");
        let (_, rest) = body.into_parts();
        assert_eq!(rest, b"NEXT");
        assert_eq!(reads.load(Ordering::SeqCst), 0, "всё тело уже лежало в буфере, сокет читать не нужно");
    }

    #[tokio::test]
    async fn empty_bodies_never_touch_the_socket() {
        for framing in [Framing::None, Framing::Length(0)] {
            let (reader, reads) = Trickle::new(b"NEXT", 4);
            let mut body = BodyReader::new(reader, Vec::new(), framing, 100).unwrap();
            assert!(body.next_chunk().await.unwrap().is_none());
            assert_eq!(reads.load(Ordering::SeqCst), 0, "{framing:?}: у запроса без тела читать нечего, байты принадлежат следующему запросу");
        }
    }

    #[tokio::test]
    async fn truncated_body_is_an_error() {
        let (reader, _) = Trickle::new(b"abc", 2);
        let mut body = BodyReader::new(reader, Vec::new(), Framing::Length(10), 100).unwrap();
        let err = collect(&mut body).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "клиент оборвал загрузку на полпути");
    }

    #[tokio::test]
    async fn declared_length_over_the_limit_fails_before_reading() {
        let taken = Arc::new(AtomicUsize::new(0));
        let reader = Endless { taken: taken.clone() };
        match BodyReader::new(reader, Vec::new(), Framing::Length(5000), 1000) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("тело заявлено больше лимита: отказать нужно сразу, не принимая ни байта"),
        }
        assert_eq!(taken.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn chunked_in_any_slicing() {
        let wire = b"4;ext=1\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\nTrailer: x\r\n\r\nNEXT";
        for step in [1, 2, 3, 5, 100] {
            let (reader, _) = Trickle::new(wire, step);
            let mut body = BodyReader::new(reader, Vec::new(), Framing::Chunked, 1000).unwrap();
            let (all, _) = collect(&mut body).await.unwrap();
            assert_eq!(all, b"Wikipedia in\r\n\r\nchunks.", "порции по {step} байт");
            let (mut reader, mut rest) = body.into_parts();
            reader.read_to_end(&mut rest).await.unwrap();
            assert_eq!(rest, b"NEXT", "после трейлеров остаётся чужое (порции по {step})");
        }
    }

    #[tokio::test]
    async fn huge_chunked_body_streams() {
        let mut wire = Vec::new();
        for _ in 0..48 {
            wire.extend_from_slice(b"10000\r\n");
            wire.extend_from_slice(&vec![b'z'; 0x10000]);
            wire.extend_from_slice(b"\r\n");
        }
        wire.extend_from_slice(b"0\r\n\r\n");
        let (reader, _) = Trickle::new(&wire, 8192);
        let mut body = BodyReader::new(reader, Vec::new(), Framing::Chunked, 4 << 20).unwrap();
        let (all, biggest) = collect(&mut body).await.unwrap();
        assert_eq!(all.len(), 48 * 0x10000);
        assert!(biggest <= 4096, "кусок в 64 КиБ нужно отдавать частями, а не копить: максимальный кусок {biggest}");
    }

    #[tokio::test]
    async fn chunked_limit_stops_the_flood_early() {
        let taken = Arc::new(AtomicUsize::new(0));
        let reader = Endless { taken: taken.clone() };
        let mut body = BodyReader::new(reader, b"FFFFFF\r\n".to_vec(), Framing::Chunked, 1000).unwrap();
        let err = body.next_chunk().await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "заявленный размер куска превышает лимит");
        assert!(taken.load(Ordering::SeqCst) < 10_000, "после заявки на 16 МБ нужно отказать сразу, а не принимать поток дальше");
    }

    #[tokio::test]
    async fn chunked_limit_is_a_sum() {
        let wire = [b"258\r\n".as_slice(), &vec![b'a'; 0x258], b"\r\n258\r\n", &vec![b'b'; 0x258], b"\r\n0\r\n\r\n"].concat();
        let (reader, _) = Trickle::new(&wire, 300);
        let mut body = BodyReader::new(reader, Vec::new(), Framing::Chunked, 1000).unwrap();
        let err = collect(&mut body).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "по 600 байт дважды это 1200 при лимите 1000");
        let wire = [b"1F4\r\n".as_slice(), &vec![b'a'; 500], b"\r\n1F4\r\n", &vec![b'b'; 500], b"\r\n0\r\n\r\n"].concat();
        let (reader, _) = Trickle::new(&wire, 300);
        let mut body = BodyReader::new(reader, Vec::new(), Framing::Chunked, 1000).unwrap();
        assert_eq!(collect(&mut body).await.unwrap().0.len(), 1000, "ровно по лимиту ещё можно");
    }

    #[tokio::test]
    async fn broken_chunked_framing() {
        for (wire, kind) in [
            (&b"zz\r\nabc\r\n0\r\n\r\n"[..], io::ErrorKind::InvalidData),
            (&b"3\r\nabcXX0\r\n\r\n"[..], io::ErrorKind::InvalidData),
            (&b"3\r\nab"[..], io::ErrorKind::UnexpectedEof),
            (&b"ffffffffffffffffff\r\nx"[..], io::ErrorKind::InvalidData),
            (&b"\r\n"[..], io::ErrorKind::InvalidData),
        ] {
            let (reader, _) = Trickle::new(wire, 4);
            let mut body = BodyReader::new(reader, Vec::new(), Framing::Chunked, 1000).unwrap();
            let err = collect(&mut body).await.unwrap_err();
            assert_eq!(err.kind(), kind, "ввод {:?}", String::from_utf8_lossy(wire));
        }
    }
}
