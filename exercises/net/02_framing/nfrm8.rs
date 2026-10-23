// Тот же формат кадра, но теперь на входе чужие. Длина из заголовка это то, что написал
// собеседник, а не то, чему можно верить: клиент может написать 4 ГБ, прислать ещё три байта
// и ждать, пока сервер выделит память. Лимит max задан вызывающим кодом, длина больше лимита
// это InvalidData. Правило то же, что и для строк: память под тело не выделяется заранее на
// чужое слово.
// TODO: Допиши read_frame_limited. Формат и ошибки обрыва те же, что в предыдущей задаче.

use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_frame_limited<R: AsyncRead + Unpin>(reader: &mut R, max: usize) -> io::Result<Option<Vec<u8>>> {
    todo!()
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
