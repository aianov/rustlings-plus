// Чат-бот читает входящие сообщения брокера построчно. Пока тексты короткие, всё работает. Но
// пришло уведомление с абзацами: оно обрезалось на первом переносе, а следующий разбор споткнулся
// об остаток и разъехался. Бинарное вложение вообще роняет чтение ошибкой. Цель: каждое
// сообщение из потока читается ровно в своих границах, что бы ни лежало внутри, а оборванный
// поток и неверная длина дают ошибку, а не молчаливую порчу следующих сообщений.


use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub subject: String,
    pub payload: Vec<u8>,
}

fn invalid(text: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.to_string())
}

pub async fn read_message<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<Message>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(None);
    }
    let words: Vec<&str> = line.trim_end().split(' ').collect();
    let [verb, subject, _sid, len] = words.as_slice() else {
        return Err(invalid("ожидали MSG <тема> <sid> <длина>"));
    };
    if *verb != "MSG" {
        return Err(invalid("ожидали MSG"));
    }
    let len: usize = len.parse().map_err(|_| invalid("длина не число"))?;
    let mut body = String::new();
    reader.read_line(&mut body).await?;
    let payload = body.trim_end_matches("\r\n").as_bytes().to_vec();
    if payload.len() != len {
        return Err(invalid("длина тела не сходится"));
    }
    let _ = reader.read_u8().await;
    Ok(Some(Message { subject: subject.to_string(), payload }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader, duplex};
    use tokio::time::timeout;

    async fn read_all(bytes: &[u8]) -> Vec<io::Result<Option<Message>>> {
        let mut reader = BufReader::new(bytes);
        let mut out = Vec::new();
        for _ in 0..8 {
            let item = read_message(&mut reader).await;
            let stop = !matches!(item, Ok(Some(_)));
            out.push(item);
            if stop {
                break;
            }
        }
        out
    }

    #[tokio::test]
    async fn plain_messages_one_after_another() {
        let got = read_all(b"MSG a.b 1 3\r\nabc\r\nMSG c.d 1 2\r\nhi\r\n").await;
        assert_eq!(got.len(), 3, "два сообщения и конец потока: {got:?}");
        assert_eq!(got[0].as_ref().unwrap(), &Some(Message { subject: "a.b".to_string(), payload: b"abc".to_vec() }), "первое сообщение");
        assert_eq!(got[1].as_ref().unwrap(), &Some(Message { subject: "c.d".to_string(), payload: b"hi".to_vec() }), "второе сообщение");
        assert!(matches!(got[2], Ok(None)), "чистый конец потока на границе сообщения это Ok(None)");
    }

    #[tokio::test]
    async fn payload_with_line_breaks_stays_whole() {
        let text = b"first paragraph\r\n\r\nsecond paragraph\r\n";
        let mut wire = format!("MSG notes.new 4 {}\r\n", text.len()).into_bytes();
        wire.extend_from_slice(text);
        wire.extend_from_slice(b"\r\nMSG notes.new 4 2\r\nok\r\n");
        let got = read_all(&wire).await;
        assert_eq!(got[0].as_ref().unwrap().as_ref().unwrap().payload, text, "абзацы с \\r\\n внутри должны прийти целиком");
        assert_eq!(got[1].as_ref().unwrap().as_ref().unwrap().payload, b"ok", "и следующее сообщение не должно разъехаться");
    }

    #[tokio::test]
    async fn binary_and_empty_payloads() {
        let mut wire = b"MSG file.chunk 1 5\r\n".to_vec();
        wire.extend_from_slice(&[0xff, 0xfe, 0x00, b'\r', 0x80]);
        wire.extend_from_slice(b"\r\nMSG empty 1 0\r\n\r\nMSG last 1 1\r\nz\r\n");
        let got = read_all(&wire).await;
        assert_eq!(got[0].as_ref().unwrap().as_ref().unwrap().payload, [0xff, 0xfe, 0x00, b'\r', 0x80], "бинарное тело это не текст");
        assert_eq!(got[1].as_ref().unwrap().as_ref().unwrap().payload, b"", "пустое тело");
        assert_eq!(got[2].as_ref().unwrap().as_ref().unwrap().payload, b"z", "и дальше всё на месте");
    }

    #[tokio::test]
    async fn bytes_arriving_in_tiny_pieces() {
        let (mut tx, rx) = duplex(8);
        let wire = b"MSG slow.pipe 3 12\r\nhello\r\nworld\r\nMSG slow.pipe 3 1\r\n!\r\n".to_vec();
        let writer = tokio::spawn(async move {
            for piece in wire.chunks(3) {
                tx.write_all(piece).await.unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        let mut reader = BufReader::new(rx);
        let first = timeout(Duration::from_secs(3), read_message(&mut reader)).await.expect("не дождались").unwrap().unwrap();
        assert_eq!(first.payload, b"hello\r\nworld", "кусками по 3 байта сообщение собирается целиком");
        let second = timeout(Duration::from_secs(3), read_message(&mut reader)).await.expect("не дождались").unwrap().unwrap();
        assert_eq!(second.payload, b"!", "второе сообщение");
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn cut_stream_and_wrong_length_are_errors() {
        let got = read_all(b"MSG a 1 10\r\nshort").await;
        let error = got[0].as_ref().expect_err("поток оборвался посреди тела: это ошибка, а не сообщение");
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof, "оборванное тело это UnexpectedEof");
        let got = read_all(b"MSG a 1 3\r\nabcde\r\nMSG a 1 1\r\nx\r\n").await;
        let error = got[0].as_ref().expect_err("длина 3, а после тела не \\r\\n: ошибка");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "неверная длина это InvalidData: {error}");
    }
}
