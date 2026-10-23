// Чтобы проверить протокол на «плохой» сети, нужен приёмник, который принимает не всё, что
// ему дали, а по кусочку, как перегруженный сокет. Вставь такую обёртку между кодом и
// настоящим приёмником и увидишь, на чём спотыкается код, написанный с расчётом на лучшее.
// TODO: Реализуй `AsyncWrite` для `Chopper`: `poll_write` передаёт вложенному приёмнику не
// больше `max` байт за раз и считает вызовы в `calls`. `poll_flush` и `poll_shutdown`
// должны дойти до вложенного приёмника.
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

struct Chopper<W> {
    inner: W,
    max: usize,
    calls: usize,
}

impl<W> Chopper<W> {
    fn new(inner: W, max: usize) -> Chopper<W> {
        Chopper { inner, max, calls: 0 }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Chopper<W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let _ = (cx, buf);
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let _ = cx;
        todo!()
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let _ = cx;
        todo!()
    }
}

#[tokio::main]
async fn main() {
    use tokio::io::AsyncWriteExt;
    let mut chopper = Chopper::new(Vec::new(), 3);
    chopper.write_all(b"0123456789").await.unwrap();
    println!("{} вызовов, {:?}", chopper.calls, chopper.inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[derive(Default)]
    struct Probe {
        data: Vec<u8>,
        flushes: u32,
        shut: bool,
    }

    impl AsyncWrite for Probe {
        fn poll_write(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            self.data.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.flushes += 1;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.shut = true;
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn write_takes_at_most_max() {
        let mut chopper = Chopper::new(Probe::default(), 3);
        let n = chopper.write(b"abcdefgh").await.unwrap();
        assert_eq!(n, 3, "за один вызов write обёртка принимает не больше max байт");
        assert_eq!(chopper.inner.data, b"abc", "вложенному приёмнику должны уйти ровно эти три байта");
    }

    #[tokio::test]
    async fn write_all_still_delivers_everything() {
        let mut chopper = Chopper::new(Probe::default(), 3);
        chopper.write_all(b"0123456789").await.unwrap();
        assert_eq!(chopper.inner.data, b"0123456789", "write_all должен довести всё через повторные записи");
        assert_eq!(chopper.calls, 4, "десять байт по три: четыре вызова poll_write");
    }

    #[tokio::test]
    async fn flush_and_shutdown_reach_the_inner_sink() {
        let mut chopper = Chopper::new(Probe::default(), 2);
        chopper.flush().await.unwrap();
        chopper.shutdown().await.unwrap();
        assert_eq!(chopper.inner.flushes, 1, "flush обязан дойти до вложенного приёмника: за ним может быть буфер");
        assert!(chopper.inner.shut, "shutdown обязан дойти до вложенного приёмника");
    }
}
