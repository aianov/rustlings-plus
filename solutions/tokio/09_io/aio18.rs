use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

struct Counted<R> {
    inner: R,
    count: u64,
}

impl<R> Counted<R> {
    fn new(inner: R) -> Counted<R> {
        Counted { inner, count: 0 }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Counted<R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            self.count += (buf.filled().len() - before) as u64;
        }
        poll
    }
}

#[tokio::main]
async fn main() {
    use tokio::io::AsyncReadExt;
    let mut counted = Counted::new(&b"hello"[..]);
    let mut sink = Vec::new();
    counted.read_to_end(&mut sink).await.unwrap();
    println!("{}", counted.count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::poll_fn;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};
    use tokio::time::sleep;

    #[tokio::test]
    async fn counts_a_full_read() {
        let mut counted = Counted::new(&b"hello world"[..]);
        let mut out = Vec::new();
        counted.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, b"hello world", "данные должны проходить через обёртку без изменений");
        assert_eq!(counted.count, 11, "счётчик должен показать все прочитанные байты");
    }

    #[tokio::test(start_paused = true)]
    async fn counts_piece_by_piece() {
        let (mut tx, rx) = duplex(64);
        tokio::spawn(async move {
            for part in ["ab", "cde", "f"] {
                tx.write_all(part.as_bytes()).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        let mut counted = Counted::new(rx);
        let mut buf = [0u8; 16];
        let n = counted.read(&mut buf).await.unwrap();
        assert_eq!(counted.count, n as u64, "после первого чтения счётчик равен прочитанному куску");
        let mut rest = Vec::new();
        counted.read_to_end(&mut rest).await.unwrap();
        assert_eq!(counted.count, 6, "в сумме прочитано шесть байт");
    }

    #[tokio::test]
    async fn prefilled_buffer_is_not_counted() {
        let mut counted = Counted::new(&b"xyz"[..]);
        let mut storage = [0u8; 8];
        let mut buf = ReadBuf::new(&mut storage);
        buf.put_slice(b"ab");
        poll_fn(|cx| Pin::new(&mut counted).poll_read(cx, &mut buf)).await.unwrap();
        assert_eq!(buf.filled(), b"abxyz", "обёртка дописывает в конец уже заполненного буфера");
        assert_eq!(counted.count, 3, "считать надо только байты, добавленные этим чтением, а не весь буфер");
    }

    #[tokio::test]
    async fn pending_does_not_count() {
        let (_tx, rx) = duplex(64);
        let mut counted = Counted::new(rx);
        let mut buf = [0u8; 4];
        let waited = tokio::time::timeout(Duration::from_millis(20), counted.read(&mut buf)).await;
        assert!(waited.is_err(), "данных нет: чтение должно ждать");
        assert_eq!(counted.count, 0, "ожидание ничего не читает");
    }
}
