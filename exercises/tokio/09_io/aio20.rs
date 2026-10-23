// Иногда нужно «посмотреть, есть ли данные прямо сейчас», не ожидая: опрос в цикле игры, в
// кадре отрисовки, перед тяжёлой работой. Обычный `read` в такой ситуации встанет и будет
// ждать. Зато под ним лежит `poll_read`, который умеет отвечать «пока нет».
// TODO: Напиши `try_read_now`: если данные есть, прочитай их в `buf` и верни `Some(n)` (ноль
// значит конец потока), а если их сейчас нет, верни `None` немедленно, не дожидаясь.
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::Poll;
use tokio::io::{AsyncRead, ReadBuf};

async fn try_read_now<R: AsyncRead + Unpin>(source: &mut R, buf: &mut [u8]) -> io::Result<Option<usize>> {
    let _ = (source, buf);
    todo!()
}

#[tokio::main]
async fn main() {
    let (_tx, mut rx) = tokio::io::duplex(64);
    let mut buf = [0u8; 8];
    println!("{:?}", try_read_now(&mut rx, &mut buf).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncWriteExt};
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn nothing_yet_means_none_right_away() {
        let (_tx, mut rx) = duplex(64);
        let mut buf = [0u8; 8];
        let got = timeout(Duration::from_millis(1), try_read_now(&mut rx, &mut buf)).await;
        let got = got.expect("try_read_now не должна ждать, когда данных нет").unwrap();
        assert_eq!(got, None, "данных нет: ответ None");
    }

    #[tokio::test]
    async fn data_is_returned() {
        let (mut tx, mut rx) = duplex(64);
        tx.write_all(b"hi").await.unwrap();
        let mut buf = [0u8; 8];
        let got = try_read_now(&mut rx, &mut buf).await.unwrap();
        assert_eq!(got, Some(2), "два байта уже лежат в потоке");
        assert_eq!(&buf[..2], b"hi", "и они должны оказаться в буфере");
    }

    #[tokio::test]
    async fn closed_stream_is_some_zero() {
        let (tx, mut rx) = duplex(64);
        drop(tx);
        let mut buf = [0u8; 8];
        assert_eq!(try_read_now(&mut rx, &mut buf).await.unwrap(), Some(0), "конец потока это Some(0), а не None");
    }
}
