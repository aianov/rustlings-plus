use std::io;
use tokio::io::{AsyncWrite, AsyncWriteExt};

async fn send_frames<W: AsyncWrite + Unpin>(sink: &mut W, frames: &[&[u8]]) -> io::Result<()> {
    for frame in frames {
        sink.write_all(&[frame.len() as u8]).await?;
        sink.write_all(frame).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut out = Vec::new();
    send_frames(&mut out, &[b"hi", b"there"]).await.unwrap();
    println!("{out:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncReadExt};

    #[tokio::test]
    async fn small_buffer_loses_nothing() {
        let (mut client, mut server) = duplex(4);
        let reader = tokio::spawn(async move {
            let mut got = Vec::new();
            server.read_to_end(&mut got).await.unwrap();
            got
        });
        send_frames(&mut client, &[b"0123456789", b"xyz"]).await.unwrap();
        drop(client);
        let got = reader.await.unwrap();
        let mut expected = vec![10];
        expected.extend_from_slice(b"0123456789");
        expected.push(3);
        expected.extend_from_slice(b"xyz");
        assert_eq!(got, expected, "получатель увидел не всё: write принял только часть кадра");
    }

    #[tokio::test]
    async fn plain_vec_still_works() {
        let mut out = Vec::new();
        send_frames(&mut out, &[b"ab"]).await.unwrap();
        assert_eq!(out, vec![2, b'a', b'b'], "в обычный Vec кадр должен лечь целиком");
    }
}
