use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_exact_by_hand<R: AsyncRead + Unpin>(source: &mut R, buf: &mut [u8]) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = source.read(&mut buf[filled..]).await?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        filled += n;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut source = &b"hello"[..];
    let mut buf = [0; 5];
    read_exact_by_hand(&mut source, &mut buf).await.unwrap();
    println!("{}", String::from_utf8_lossy(&buf));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncWriteExt, DuplexStream};
    use tokio::time::sleep;

    fn chunked(parts: &'static [&'static str]) -> DuplexStream {
        let (mut tx, rx) = duplex(64);
        tokio::spawn(async move {
            for part in parts {
                tx.write_all(part.as_bytes()).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });
        rx
    }

    #[tokio::test(start_paused = true)]
    async fn collects_pieces_into_buffer() {
        let mut source = chunked(&["ab", "c", "defg"]);
        let mut buf = [0u8; 6];
        read_exact_by_hand(&mut source, &mut buf).await.unwrap();
        assert_eq!(&buf, b"abcdef", "буфер должен заполниться из нескольких кусков");
        let mut rest = Vec::new();
        source.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"g", "лишнего читать нельзя: байт после буфера должен остаться в потоке");
    }

    #[tokio::test(start_paused = true)]
    async fn early_end_is_unexpected_eof() {
        let mut source = chunked(&["ab"]);
        let mut buf = [0u8; 4];
        let err = read_exact_by_hand(&mut source, &mut buf).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "поток кончился на полпути: ждём UnexpectedEof");
    }

    #[tokio::test]
    async fn empty_buffer_is_fine_on_empty_stream() {
        let mut source = &b""[..];
        let mut buf = [0u8; 0];
        let result = read_exact_by_hand(&mut source, &mut buf).await;
        assert!(result.is_ok(), "пустому буферу читать нечего: это успех, а не конец потока");
    }
}
