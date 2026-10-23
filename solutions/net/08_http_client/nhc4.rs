use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

type Headers = Vec<(String, String)>;

#[allow(dead_code)]
fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

#[derive(Debug, Clone, PartialEq)]
struct Head {
    minor: u8,
    status: u16,
    reason: String,
    headers: Headers,
    len: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

#[allow(dead_code)]
const MAX_HEAD: usize = 16 * 1024;

impl Head {
    #[allow(dead_code)]
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    #[allow(dead_code)]
    fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.all("connection") {
            for token in value.split(',') {
                close |= token.trim().eq_ignore_ascii_case("close");
                keep |= token.trim().eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.minor == 1)
    }
}


#[derive(Debug, Clone, PartialEq)]
struct Body {
    bytes: Vec<u8>,
    reusable: bool,
}

async fn fill<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<usize> {
    let mut chunk = [0u8; 4096];
    let n = reader.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n)
}

fn invalid<T>(why: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, why.to_string()))
}

async fn take_line<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<String> {
    loop {
        if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n") {
            let line: Vec<u8> = buf.drain(..pos + 2).take(pos).collect();
            return String::from_utf8(line).or_else(|_| invalid("строка не в utf-8"));
        }
        if buf.len() > 4096 {
            return invalid("слишком длинная строка");
        }
        if fill(reader, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
}

async fn read_body<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>, head: &Head, max: usize) -> io::Result<Body> {
    let encodings = head.all("transfer-encoding");
    let chunked = encodings.last().is_some_and(|v| v.rsplit(',').next().is_some_and(|t| t.trim().eq_ignore_ascii_case("chunked")));
    let mut lengths = Vec::new();
    for value in head.all("content-length") {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return invalid("плохой Content-Length");
            }
            lengths.push(part.parse::<usize>().or_else(|_| invalid("Content-Length слишком велик"))?);
        }
    }
    if lengths.windows(2).any(|w| w[0] != w[1]) {
        return invalid("разные Content-Length");
    }
    if chunked {
        let mut bytes = Vec::new();
        loop {
            let line = take_line(reader, buf).await?;
            let size_text = line.split(';').next().unwrap_or("").trim();
            if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                return invalid("плохой размер куска");
            }
            let size = usize::from_str_radix(size_text, 16).or_else(|_| invalid("размер куска слишком велик"))?;
            if size == 0 {
                while !take_line(reader, buf).await?.is_empty() {}
                return Ok(Body { bytes, reusable: lengths.is_empty() });
            }
            if size > max - bytes.len().min(max) {
                return invalid("тело больше лимита");
            }
            while buf.len() < size + 2 {
                if fill(reader, buf).await? == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            bytes.extend_from_slice(&buf[..size]);
            if &buf[size..size + 2] != b"\r\n" {
                return invalid("после куска нет CRLF");
            }
            buf.drain(..size + 2);
        }
    }
    if let Some(&n) = lengths.first().filter(|_| encodings.is_empty()) {
        if n > max {
            return invalid("тело больше лимита");
        }
        while buf.len() < n {
            if fill(reader, buf).await? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
        return Ok(Body { bytes: buf.drain(..n).collect(), reusable: true });
    }
    loop {
        if buf.len() > max {
            return invalid("тело больше лимита");
        }
        if fill(reader, buf).await? == 0 {
            return Ok(Body { bytes: std::mem::take(buf), reusable: false });
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    struct Trickle {
        data: Vec<u8>,
        pos: usize,
        step: usize,
        hold_open: bool,
    }

    impl Trickle {
        fn new(data: &[u8], step: usize, hold_open: bool) -> Trickle {
            Trickle { data: data.to_vec(), pos: 0, step, hold_open }
        }
    }

    impl AsyncRead for Trickle {
        fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, out: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            if self.pos >= self.data.len() {
                return if self.hold_open { Poll::Pending } else { Poll::Ready(Ok(())) };
            }
            let n = self.step.min(out.remaining()).min(self.data.len() - self.pos);
            let end = self.pos + n;
            out.put_slice(&self.data[self.pos..end]);
            self.pos = end;
            Poll::Ready(Ok(()))
        }
    }

    fn head(headers: &[(&str, &str)]) -> Head {
        Head {
            minor: 1,
            status: 200,
            reason: "OK".into(),
            headers: headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
            len: 0,
        }
    }

    async fn run(headers: &[(&str, &str)], wire: &[u8], step: usize, hold_open: bool) -> (io::Result<Body>, Vec<u8>) {
        let mut reader = Trickle::new(wire, step, hold_open);
        let mut buf = Vec::new();
        let result = timeout(Duration::from_secs(2), read_body(&mut reader, &mut buf, &head(headers), 1 << 20))
            .await
            .expect("read_body завис: он ждёт конца потока там, где длина тела уже известна");
        let mut left = buf;
        left.extend_from_slice(&reader.data[reader.pos..]);
        (result, left)
    }

    #[tokio::test]
    async fn content_length_does_not_wait_for_eof() {
        let (body, left) = run(&[("Content-Length", "5")], b"helloNEXT", 3, true).await;
        let body = body.unwrap();
        assert_eq!(body.bytes, b"hello");
        assert!(body.reusable, "длина известна и тело прочитано: соединение можно использовать снова");
        assert_eq!(left, b"NEXT", "то, что пришло после тела, принадлежит следующему ответу и должно остаться в буфере");
    }

    #[tokio::test]
    async fn body_may_already_sit_in_the_buffer() {
        let mut reader = Trickle::new(b"", 1, true);
        let mut buf = b"abcdefXYZ".to_vec();
        let body = timeout(Duration::from_secs(2), read_body(&mut reader, &mut buf, &head(&[("Content-Length", "6")]), 100)).await.unwrap().unwrap();
        assert_eq!(body.bytes, b"abcdef", "часть тела приехала вместе с головой и уже лежит в буфере");
        assert_eq!(buf, b"XYZ");
    }

    #[tokio::test]
    async fn zero_length_body() {
        let (body, left) = run(&[("Content-Length", "0")], b"NEXT", 1, true).await;
        assert_eq!(body.unwrap().bytes, b"");
        assert_eq!(left, b"NEXT");
    }

    #[tokio::test]
    async fn short_body_is_an_error() {
        let (body, _) = run(&[("Content-Length", "10")], b"abc", 2, false).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::UnexpectedEof, "соединение закрылось раньше, чем пришли 10 байтов");
    }

    #[tokio::test]
    async fn chunked_in_any_slicing() {
        let wire = b"4;ext=1\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\nTrailer: x\r\n\r\nNEXT";
        for step in [1, 2, 3, 7, 1000] {
            let (body, left) = run(&[("Transfer-Encoding", "chunked")], wire, step, true).await;
            let body = body.unwrap();
            assert_eq!(body.bytes, b"Wikipedia in\r\n\r\nchunks.", "порции по {step} байт");
            assert!(body.reusable);
            assert_eq!(left, b"NEXT", "после нулевого куска и трейлеров остаток принадлежит следующему ответу (порции по {step})");
        }
    }

    #[tokio::test]
    async fn chunked_is_found_in_a_list_and_without_case() {
        let (body, _) = run(&[("transfer-encoding", "gzip, Chunked")], b"3\r\nabc\r\n0\r\n\r\n", 5, true).await;
        assert_eq!(body.unwrap().bytes, b"abc");
    }

    #[tokio::test]
    async fn broken_chunks() {
        let (body, _) = run(&[("Transfer-Encoding", "chunked")], b"zz\r\nabc\r\n0\r\n\r\n", 4, false).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::InvalidData, "размер куска не из шестнадцатеричных цифр");
        let (body, _) = run(&[("Transfer-Encoding", "chunked")], b"3\r\nabcXX0\r\n\r\n", 4, false).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::InvalidData, "после данных куска обязателен CRLF");
        let (body, _) = run(&[("Transfer-Encoding", "chunked")], b"3\r\nab", 4, false).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::UnexpectedEof, "поток кончился посреди куска");
        let (body, _) = run(&[("Transfer-Encoding", "chunked")], b"ffffffffffffffffff\r\nx", 4, false).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::InvalidData, "размер не помещается в usize: это ошибка, а не паника");
    }

    #[tokio::test]
    async fn until_close_reads_everything_and_forbids_reuse() {
        let (body, _) = run(&[], b"all of it, no length given", 4, false).await;
        let body = body.unwrap();
        assert_eq!(body.bytes, b"all of it, no length given");
        assert!(!body.reusable, "тело без длины кончается закрытием соединения: использовать его снова нельзя");
    }

    #[tokio::test]
    async fn bad_lengths() {
        for value in ["abc", "-1", "+5", "1.5", "", "99999999999999999999999999"] {
            let (body, _) = run(&[("Content-Length", value)], b"hello", 5, true).await;
            assert_eq!(body.unwrap_err().kind(), io::ErrorKind::InvalidData, "Content-Length: {value:?} не число");
        }
        let (body, _) = run(&[("Content-Length", "5"), ("Content-Length", "6")], b"hello!", 5, true).await;
        assert_eq!(body.unwrap_err().kind(), io::ErrorKind::InvalidData, "два разных Content-Length: верить нельзя ни одному");
        let (body, _) = run(&[("Content-Length", "5"), ("Content-Length", "5")], b"hello", 5, true).await;
        assert_eq!(body.unwrap().bytes, b"hello", "одинаковые повторы безвредны");
        let (body, _) = run(&[("Content-Length", "5, 5")], b"hello", 5, true).await;
        assert_eq!(body.unwrap().bytes, b"hello");
    }

    #[tokio::test]
    async fn chunked_wins_over_length_but_poisons_the_connection() {
        let (body, _) = run(&[("Content-Length", "100"), ("Transfer-Encoding", "chunked")], b"2\r\nok\r\n0\r\n\r\n", 5, true).await;
        let body = body.unwrap();
        assert_eq!(body.bytes, b"ok", "если есть оба заголовка, длина тела определяется chunked");
        assert!(!body.reusable, "оба заголовка сразу это признак подмены: соединение после такого ответа закрываем");
    }

    #[tokio::test]
    async fn limit_is_enforced_before_the_body_arrives() {
        let mut reader = Trickle::new(b"", 1, true);
        let mut buf = Vec::new();
        let r = timeout(Duration::from_secs(2), read_body(&mut reader, &mut buf, &head(&[("Content-Length", "5000")]), 100)).await.expect("не жди тело, которое уже не примешь");
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::InvalidData);
        let (big, _) = run(&[("Transfer-Encoding", "chunked")], &[b"1FFFFF\r\n".as_slice(), &[b'a'; 20]].concat(), 8, true).await;
        assert_eq!(big.unwrap_err().kind(), io::ErrorKind::InvalidData, "размер куска сам по себе больше лимита");
    }
}
