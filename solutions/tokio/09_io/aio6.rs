use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, PartialEq)]
enum BodyError {
    TooLarge,
    Io(io::ErrorKind),
}

async fn read_body<R: AsyncRead + Unpin>(source: R, limit: u64) -> Result<Vec<u8>, BodyError> {
    let mut body = Vec::new();
    source
        .take(limit + 1)
        .read_to_end(&mut body)
        .await
        .map_err(|e| BodyError::Io(e.kind()))?;
    if body.len() as u64 > limit {
        return Err(BodyError::TooLarge);
    }
    Ok(body)
}

#[tokio::main]
async fn main() {
    println!("{:?}", read_body(&b"hello"[..], 10).await);
    println!("{:?}", read_body(tokio::io::repeat(b'x'), 10).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn small_body_is_returned() {
        assert_eq!(read_body(&b"hello"[..], 10).await, Ok(b"hello".to_vec()), "тело в пределах лимита вернулось целиком");
    }

    #[tokio::test]
    async fn exactly_limit_is_allowed() {
        let body = read_body(&b"0123456789"[..], 10).await;
        assert_eq!(body, Ok(b"0123456789".to_vec()), "тело ровно на лимит допустимо");
    }

    #[tokio::test]
    async fn one_byte_over_is_too_large() {
        let body = read_body(&b"0123456789!"[..], 10).await;
        assert_eq!(body, Err(BodyError::TooLarge), "на один байт больше лимита: это уже TooLarge, а не обрезанное тело");
    }

    #[tokio::test]
    async fn endless_source_is_cut() {
        let source = tokio::io::repeat(b'x').take(1 << 20);
        let body = read_body(source, 1024).await;
        assert_eq!(body, Err(BodyError::TooLarge), "источник намного больше лимита: нужен TooLarge, а не мегабайт в памяти");
    }
}
