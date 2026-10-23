// Сервер читает команды построчно и проверяет, что строка не длиннее max байт. Проверка есть,
// тесты на обычных строках зелёные. А потом злой клиент присылает гигабайт без единого
// перевода строки, и сервер умирает от нехватки памяти ещё до проверки. Найди, в чём дело.
// Лимит должен работать, пока строка ещё читается: память не должна расти больше max. И помни,
// что данные приходят кусками как попало: '\r' и '\n' одной строки могут прийти в разных чтениях.

use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

async fn read_line_limited<R: AsyncBufRead + Unpin>(reader: &mut R, max: usize) -> io::Result<Option<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(None);
    }
    if line.trim_end().len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "строка длиннее лимита"));
    }
    Ok(Some(line.trim_end().to_owned()))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut input = tokio::io::BufReader::new(tokio::io::stdin());
    while let Some(line) = read_line_limited(&mut input, 1024).await? {
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, BufReader};

    #[tokio::test]
    async fn normal_lines_pass() {
        let mut r = BufReader::new(&b"abc\r\nde\nf"[..]);
        assert_eq!(read_line_limited(&mut r, 3).await.unwrap().as_deref(), Some("abc"), "строка ровно в лимит проходит, \\r не считается");
        assert_eq!(read_line_limited(&mut r, 3).await.unwrap().as_deref(), Some("de"));
        let err = read_line_limited(&mut r, 3).await.expect_err("хвост без перевода строки это обрыв");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn too_long_line_is_rejected() {
        let mut r = BufReader::new(&b"abcd\nok\n"[..]);
        let err = read_line_limited(&mut r, 3).await.expect_err("4 байта при лимите 3");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "слишком длинная строка это InvalidData");
    }

    #[tokio::test]
    async fn endless_line_is_cut_early() {
        let mut r = BufReader::new(tokio::io::repeat(b'a').take(50_000_000));
        let err = read_line_limited(&mut r, 1024)
            .await
            .expect_err("50 МБ без перевода строки при лимите 1 КБ: это должна быть ошибка");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "ждали InvalidData, получили {err}");
        let left = r.get_ref().limit();
        let consumed = 50_000_000 - left;
        assert!(
            consumed < 1_000_000,
            "прочитано {consumed} байт ради строки с лимитом 1024: лимит проверяется уже после того, как строка вся в памяти"
        );
    }

    #[tokio::test]
    async fn lone_cr_before_newline_is_not_counted() {
        use tokio::io::AsyncWriteExt;
        let (mut tx, rx) = tokio::io::duplex(64);
        let mut r = BufReader::new(rx);
        tx.write_all(b"abc\r").await.unwrap();
        let feeder = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            tx.write_all(b"\n").await.unwrap();
            tx
        });
        let line = read_line_limited(&mut r, 3).await.expect("'abc' плюс '\\r' при лимите 3 это нормальная строка: '\\r' приходит раньше '\\n' и в лимит не входит");
        assert_eq!(line.as_deref(), Some("abc"));
        let _ = feeder.await;
    }
}
