// Записи в бинарном журнале разделены нулевым байтом, а внутри записи могут быть переводы
// строки. Читать такое построчно нельзя: строка по `\n` разрежет запись пополам.
// TODO: Напиши `records`: вычитай источник и верни записи без разделителя как строки.
// Пустая запись между двумя нулями это пустая строка. Последняя запись может прийти без
// завершающего нуля, её терять нельзя. Если запись не UTF-8, верни ошибку `InvalidData`.
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

async fn records<R: AsyncRead + Unpin>(source: R) -> io::Result<Vec<String>> {
    let _ = BufReader::new(source);
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", records(&b"a\0b\nc\0"[..]).await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn newline_inside_record_survives() {
        let got = records(&b"one\0two\nlines\0last"[..]).await.unwrap();
        assert_eq!(got, vec!["one", "two\nlines", "last"], "разделитель только нулевой байт, а \\n внутри записи остаётся");
    }

    #[tokio::test]
    async fn empty_records_are_kept() {
        let got = records(&b"a\0\0b\0"[..]).await.unwrap();
        assert_eq!(got, vec!["a", "", "b"], "два нуля подряд дают пустую запись, а нуль в конце новой записи не рождает");
    }

    #[tokio::test]
    async fn empty_source() {
        assert!(records(&b""[..]).await.unwrap().is_empty(), "пустой источник это ноль записей");
    }

    #[tokio::test]
    async fn bad_utf8_is_invalid_data() {
        let err = records(&[b'o', b'k', 0, 0xff, 0xfe, 0][..]).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "запись не UTF-8: ждём ошибку InvalidData");
    }
}
