use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[derive(Default)]
struct Log {
    incoming: Vec<u8>,
    outgoing: Vec<u8>,
}

struct Tap<S> {
    inner: S,
    log: Arc<Mutex<Log>>,
}

impl<S> Tap<S> {
    fn new(inner: S) -> (Tap<S>, Arc<Mutex<Log>>) {
        let log = Arc::new(Mutex::new(Log::default()));
        (Tap { inner, log: log.clone() }, log)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Tap<S> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            this.log.lock().unwrap().incoming.extend_from_slice(&buf.filled()[before..]);
        }
        poll
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Tap<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = poll {
            this.log.lock().unwrap().outgoing.extend_from_slice(&buf[..n]);
        }
        poll
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let (client, _server) = tokio::io::duplex(64);
    let (mut tap, log) = Tap::new(client);
    tap.write_all(b"hello").await?;
    println!("отправлено: {:?}", log.lock().unwrap().outgoing);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};
    use tokio::time::timeout;

    #[tokio::test]
    async fn records_both_directions() {
        let (near, mut far) = duplex(1024);
        let (mut tap, log) = Tap::new(near);
        tap.write_all(b"request").await.unwrap();
        far.write_all(b"reply").await.unwrap();
        let mut got = [0u8; 5];
        timeout(Duration::from_secs(3), tap.read_exact(&mut got)).await.expect("завис").unwrap();
        assert_eq!(&got, b"reply", "Tap должен отдавать данные как есть");
        let mut seen = [0u8; 7];
        far.read_exact(&mut seen).await.unwrap();
        assert_eq!(&seen, b"request", "Tap должен пропускать записанное как есть");
        let log = log.lock().unwrap();
        assert_eq!(log.outgoing, b"request", "в журнале исходящих должно быть то, что записали");
        assert_eq!(log.incoming, b"reply", "в журнале входящих должно быть то, что прочитали");
    }

    #[tokio::test]
    async fn partial_writes_are_logged_exactly_once() {
        let (near, mut far) = duplex(3);
        let (mut tap, log) = Tap::new(near);
        let reader = tokio::spawn(async move {
            let mut all = Vec::new();
            far.read_to_end(&mut all).await.unwrap();
            all
        });
        timeout(Duration::from_secs(5), tap.write_all(b"hello world")).await.expect("запись зависла").unwrap();
        tap.shutdown().await.unwrap();
        let got = timeout(Duration::from_secs(5), reader).await.expect("читатель не получил EOF: shutdown должен дойти до внутреннего потока").unwrap();
        assert_eq!(got, b"hello world", "данные дошли с искажениями");
        assert_eq!(
            log.lock().unwrap().outgoing,
            b"hello world",
            "буфер на 3 байта принимает запись кусками: в журнал нужно писать только принятые n байт, а не весь предложенный буфер"
        );
    }

    #[tokio::test]
    async fn reads_log_only_the_filled_part() {
        let (near, mut far) = duplex(1024);
        let (mut tap, log) = Tap::new(near);
        far.write_all(b"ab").await.unwrap();
        let mut buf = [0u8; 64];
        let n = timeout(Duration::from_secs(3), tap.read(&mut buf)).await.expect("завис").unwrap();
        assert_eq!(n, 2);
        assert_eq!(log.lock().unwrap().incoming, b"ab", "в журнал входящих попал не весь буфер, а только прочитанные байты");
    }
}
