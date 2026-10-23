use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_frame_limited<R: AsyncRead + Unpin>(reader: &mut R, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    if reader.read(&mut header[..1]).await? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..]).await?;
    let len = u32::from_be_bytes(header) as usize;
    if len > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("кадр в {len} байт больше лимита {max}")));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut input = tokio::io::stdin();
    while let Some(frame) = read_frame_limited(&mut input, 1 << 20).await? {
        println!("кадр из {} байт", frame.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_within_the_limit() {
        let mut input: &[u8] = &[0, 0, 0, 3, 1, 2, 3];
        assert_eq!(read_frame_limited(&mut input, 3).await.unwrap(), Some(vec![1, 2, 3]), "кадр ровно в лимит проходит");
    }

    #[tokio::test]
    async fn one_byte_over_the_limit() {
        let mut input: &[u8] = &[0, 0, 0, 4, 1, 2, 3, 4];
        let err = read_frame_limited(&mut input, 3).await.expect_err("4 байта при лимите 3");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "кадр больше лимита это InvalidData, получили {err}");
    }

    #[tokio::test]
    async fn huge_length_is_rejected_by_the_header_alone() {
        let mut input: &[u8] = &[0xff, 0xff, 0xff, 0xff, 1, 2, 3];
        let err = read_frame_limited(&mut input, 1024).await.expect_err("заявлено 4 ГБ");
        assert_eq!(
            err.kind(),
            io::ErrorKind::InvalidData,
            "заявлено 4 ГБ, а пришло 3 байта: отказ должен прийти по одному заголовку (получили {err}), не дожидаясь тела и без выделения памяти"
        );
    }

    #[tokio::test]
    async fn ends_are_still_distinguished() {
        let mut empty: &[u8] = &[];
        assert_eq!(read_frame_limited(&mut empty, 10).await.unwrap(), None, "чистый конец это None");
        let mut cut: &[u8] = &[0, 0, 0, 2, 9];
        let err = read_frame_limited(&mut cut, 10).await.expect_err("тело оборвано");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
