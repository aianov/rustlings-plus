// Читаем кадры: четыре байта длины (u32 big-endian), потом столько байтов тела. Есть два вида
// конца потока, и различать их нужно обязательно. Собеседник закрылся между кадрами: это нормальный
// конец. Закрылся посреди кадра: это обрыв, данные потеряны, и молчать об этом нельзя.
// TODO: Допиши read_frame. Чистый конец до первого байта заголовка это Ok(None). Конец в любом
// другом месте кадра это ошибка UnexpectedEof.

use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut input = tokio::io::stdin();
    while let Some(frame) = read_frame(&mut input).await? {
        println!("кадр из {} байт", frame.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn frames(mut input: &[u8]) -> io::Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        while let Some(f) = read_frame(&mut input).await? {
            out.push(f);
        }
        Ok(out)
    }

    #[tokio::test]
    async fn two_frames_then_clean_end() {
        let got = frames(&[0, 0, 0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 1, b'x']).await.unwrap();
        assert_eq!(got, vec![b"hi".to_vec(), vec![], b"x".to_vec()], "кадры должны читаться по порядку, пустой тоже кадр");
    }

    #[tokio::test]
    async fn empty_input_is_a_clean_end() {
        assert!(frames(&[]).await.unwrap().is_empty(), "пустой поток это нормальный конец без кадров");
    }

    #[tokio::test]
    async fn cut_inside_the_header() {
        for cut in 1..4 {
            let err = frames(&[0, 0, 0, 2, 1, 2][..cut]).await.expect_err("заголовок оборван");
            assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "обрыв после {cut} байт заголовка должен быть ошибкой, а не тихим концом");
        }
    }

    #[tokio::test]
    async fn cut_inside_the_body() {
        let err = frames(&[0, 0, 0, 5, b'a', b'b']).await.expect_err("тело оборвано: обещали 5 байт, пришло 2");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ждали UnexpectedEof, получили {err}");
        let err = frames(&[0, 0, 0, 1, b'a', 0, 0]).await.expect_err("второй кадр оборван в заголовке");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "обрыв после целого кадра тоже ошибка");
    }
}
