use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};

fn encode_frame(payload: &[u8]) -> io::Result<Vec<u8>> {
    let len = u32::try_from(payload.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "тело длиннее u32"))?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    writer.write_all(&encode_frame(payload)?).await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut out = tokio::io::stdout();
    write_frame(&mut out, b"hello").await?;
    println!();
    println!("{:?}", encode_frame(b"hi")?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[derive(Default)]
    struct Recorder {
        writes: Vec<Vec<u8>>,
    }

    impl AsyncWrite for Recorder {
        fn poll_write(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            self.writes.push(buf.to_vec());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn layout_of_a_frame() {
        assert_eq!(encode_frame(b"hi").unwrap(), [0, 0, 0, 2, b'h', b'i'], "заголовок 4 байта big-endian, потом тело");
        assert_eq!(encode_frame(b"").unwrap(), [0, 0, 0, 0], "пустое тело это только заголовок");
        let big = vec![7u8; 258];
        let frame = encode_frame(&big).unwrap();
        assert_eq!(&frame[..4], [0, 0, 1, 2], "258 это 0x0102: старший байт первым");
        assert_eq!(frame.len(), 262);
    }

    #[tokio::test]
    async fn frame_goes_out_in_one_write() {
        let mut rec = Recorder::default();
        write_frame(&mut rec, b"hello").await.unwrap();
        assert_eq!(rec.writes.len(), 1, "кадр ушёл {} записями: заголовок и тело нужно отдавать сокету одним куском", rec.writes.len());
        assert_eq!(rec.writes[0], [0, 0, 0, 5, b'h', b'e', b'l', b'l', b'o']);
    }

    #[tokio::test]
    async fn several_frames_stay_separate_and_in_order() {
        let mut rec = Recorder::default();
        write_frame(&mut rec, b"a").await.unwrap();
        write_frame(&mut rec, b"bc").await.unwrap();
        let all: Vec<u8> = rec.writes.concat();
        assert_eq!(all, [0, 0, 0, 1, b'a', 0, 0, 0, 2, b'b', b'c']);
    }
}
