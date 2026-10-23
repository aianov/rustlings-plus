use bytes::{Bytes, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn next_line<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut BytesMut) -> io::Result<Option<Bytes>> {
    loop {
        if let Some(i) = buf.iter().position(|b| *b == b'\n') {
            let mut line = buf.split_to(i + 1);
            line.truncate(i);
            return Ok(Some(line.freeze()));
        }
        if reader.read_buf(buf).await? == 0 {
            return if buf.is_empty() { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut input = tokio::io::stdin();
    let mut buf = BytesMut::new();
    while let Some(line) = next_line(&mut input, &mut buf).await? {
        println!("{}", String::from_utf8_lossy(&line));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, duplex};
    use tokio::time::timeout;

    #[tokio::test]
    async fn two_lines_from_one_read() {
        let mut reader = &b"a\nbb\n"[..];
        let mut buf = BytesMut::new();
        assert_eq!(next_line(&mut reader, &mut buf).await.unwrap().unwrap(), "a", "первая строка");
        assert_eq!(&buf[..], b"bb\n", "вторая строка уже пришла и должна остаться в buf, а не пропасть");
        assert_eq!(next_line(&mut reader, &mut buf).await.unwrap().unwrap(), "bb", "вторая строка берётся из buf без нового чтения");
        assert!(next_line(&mut reader, &mut buf).await.unwrap().is_none(), "чистый конец потока");
    }

    #[tokio::test]
    async fn line_split_across_reads() {
        let (mut tx, mut rx) = duplex(64);
        tokio::spawn(async move {
            for part in [&b"hel"[..], b"lo\nwor", b"ld\n"] {
                tx.write_all(part).await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let mut buf = BytesMut::new();
        let first = timeout(Duration::from_secs(3), next_line(&mut rx, &mut buf)).await.expect("завис").unwrap().unwrap();
        assert_eq!(first, "hello", "строка пришла кусками");
        let second = timeout(Duration::from_secs(3), next_line(&mut rx, &mut buf)).await.expect("завис").unwrap().unwrap();
        assert_eq!(second, "world", "начало второй строки пришло вместе с концом первой и не должно потеряться");
        assert!(timeout(Duration::from_secs(3), next_line(&mut rx, &mut buf)).await.expect("завис").unwrap().is_none());
    }

    #[tokio::test]
    async fn empty_line_and_truncated_tail() {
        let mut reader = &b"\nrest"[..];
        let mut buf = BytesMut::new();
        assert_eq!(next_line(&mut reader, &mut buf).await.unwrap().unwrap(), "", "пустая строка это тоже сообщение");
        let err = next_line(&mut reader, &mut buf).await.expect_err("«rest» без перевода строки: обрыв");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ждали UnexpectedEof, получили {err}");
    }
}
