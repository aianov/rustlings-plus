// Протокол простой: сообщение это два байта длины (big-endian), потом столько же байт тела.
// Читатель должен отличать два конца: аккуратный (поток кончился ровно между сообщениями,
// это нормально) и грязный (поток оборвался посреди заголовка или тела, это ошибка, данные
// потеряны). Сейчас оборванный заголовок читатель принимает за нормальный конец и тихо
// выдаёт неполный набор сообщений. Исправь.
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_message<R: AsyncRead + Unpin>(source: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 2];
    match source.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let mut body = vec![0u8; u16::from_be_bytes(header) as usize];
    source.read_exact(&mut body).await?;
    Ok(Some(body))
}

#[tokio::main]
async fn main() {
    let mut source = &[0u8, 2, b'h', b'i'][..];
    println!("{:?}", read_message(&mut source).await.unwrap());
    println!("{:?}", read_message(&mut source).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn all(mut bytes: &[u8]) -> io::Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        while let Some(m) = read_message(&mut bytes).await? {
            out.push(m);
        }
        Ok(out)
    }

    #[tokio::test]
    async fn clean_end_between_messages() {
        let got = all(&[0, 3, b'a', b'b', b'c', 0, 0, 0, 1, b'z']).await.unwrap();
        assert_eq!(got, vec![b"abc".to_vec(), b"".to_vec(), b"z".to_vec()], "три сообщения, среднее пустое");
    }

    #[tokio::test]
    async fn empty_stream_is_clean() {
        assert_eq!(all(&[]).await.unwrap(), Vec::<Vec<u8>>::new(), "пустой поток это ноль сообщений без ошибки");
    }

    #[tokio::test]
    async fn cut_body_is_an_error() {
        let err = all(&[0, 3, b'a', b'b']).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "тело оборвано: это ошибка UnexpectedEof");
    }

    #[tokio::test]
    async fn cut_header_is_an_error() {
        let err = all(&[0, 1, b'a', 0]).await.unwrap_err();
        assert_eq!(
            err.kind(),
            io::ErrorKind::UnexpectedEof,
            "заголовок оборвался после первого байта: это не конец, а потеря данных"
        );
    }
}
