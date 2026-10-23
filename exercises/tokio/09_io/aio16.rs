// Читатель строк для сетевого клиента. Ждёт строку и параллельно отвечает на сигналы
// таймера, поэтому его вызывают из `select!`. Беда в том, что когда выигрывает таймер, ожидание
// строки отменяется, а прочитанное к этому моменту начало строки пропадает. Строки приходят
// покалеченными. Переделай читатель так, чтобы отмена `next_line` не стоила данных.
use bytes::{Bytes, BytesMut};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

struct LineReader<R> {
    reader: R,
    buf: BytesMut,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    fn new(reader: R) -> LineReader<R> {
        LineReader { reader, buf: BytesMut::new() }
    }

    async fn next_line(&mut self) -> io::Result<Option<Bytes>> {
        let mut line = Vec::new();
        loop {
            let byte = match self.reader.read_u8().await {
                Ok(byte) => byte,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(if line.is_empty() { None } else { Some(Bytes::from(line)) });
                }
                Err(e) => return Err(e),
            };
            if byte == b'\n' {
                return Ok(Some(Bytes::from(line)));
            }
            line.push(byte);
        }
    }
}

#[tokio::main]
async fn main() {
    let mut reader = LineReader::new(&b"one\ntwo"[..]);
    while let Some(line) = reader.next_line().await.unwrap() {
        println!("{}", String::from_utf8_lossy(&line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{duplex, AsyncWriteExt};
    use tokio::time::{interval, sleep};

    #[tokio::test(start_paused = true)]
    async fn survives_cancellation_by_a_ticker() {
        let (mut tx, rx) = duplex(64);
        tokio::spawn(async move {
            for part in ["he", "llo\nwor", "ld", "\nlast"] {
                tx.write_all(part.as_bytes()).await.unwrap();
                sleep(Duration::from_millis(5)).await;
            }
        });
        let mut reader = LineReader::new(rx);
        let mut ticker = interval(Duration::from_millis(1));
        let mut lines = Vec::new();
        let mut ticks = 0;
        loop {
            tokio::select! {
                line = reader.next_line() => match line.unwrap() {
                    Some(line) => lines.push(String::from_utf8(line.to_vec()).unwrap()),
                    None => break,
                },
                _ = ticker.tick() => ticks += 1,
            }
        }
        assert!(ticks > 5, "тест не проверяет отмену: таймер должен успевать выигрывать");
        assert_eq!(lines, vec!["hello", "world", "last"], "строки пришли искалеченными: отмена выбросила уже прочитанное");
    }

    #[tokio::test]
    async fn plain_lines_and_eof() {
        let mut reader = LineReader::new(&b"a\n\nb"[..]);
        assert_eq!(reader.next_line().await.unwrap().as_deref(), Some(&b"a"[..]), "первая строка");
        assert_eq!(reader.next_line().await.unwrap().as_deref(), Some(&b""[..]), "пустая строка");
        assert_eq!(reader.next_line().await.unwrap().as_deref(), Some(&b"b"[..]), "последняя строка без \\n отдаётся на конце потока");
        assert_eq!(reader.next_line().await.unwrap(), None, "потом конец");
    }
}
