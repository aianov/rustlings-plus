// collect читает кадры (u32 длины и тело) и параллельно считает тики таймера: так сервер
// мог бы, скажем, вести статистику простоя. Пока кадры приходят целиком, всё хорошо. Но стоит
// кадру прийти медленно, двумя частями с паузой больше тика, как поток «съезжает» и кадры
// превращаются в мусор. Найди причину. Помощники read_frame и decode_frame в файле уже есть,
// какой из них годится для select!, решай сам. Ненужного после правки помощника не оставляй:
// clippy с -D warnings ругается на мёртвый код.

use bytes::{Buf, BytesMut};
use std::io;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::interval;

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    if reader.read(&mut header[..1]).await? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..]).await?;
    let mut body = vec![0u8; u32::from_be_bytes(header) as usize];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

fn decode_frame(buf: &mut BytesMut) -> Option<Vec<u8>> {
    if buf.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return None;
    }
    buf.advance(4);
    Some(buf.split_to(len).to_vec())
}

async fn collect<R: AsyncRead + Unpin>(mut reader: R, tick: Duration, expected: usize) -> io::Result<(Vec<Vec<u8>>, u32)> {
    let mut ticker = interval(tick);
    let mut frames = Vec::new();
    let mut ticks = 0;
    while frames.len() < expected {
        tokio::select! {
            frame = read_frame(&mut reader) => match frame? {
                Some(f) => frames.push(f),
                None => break,
            },
            _ = ticker.tick() => ticks += 1,
        }
    }
    Ok((frames, ticks))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (frames, ticks) = collect(tokio::io::stdin(), Duration::from_secs(1), 10).await?;
    println!("кадров {}, тиков {ticks}", frames.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncWriteExt, duplex};
    use tokio::time::sleep;

    fn frame(body: &[u8]) -> Vec<u8> {
        let mut f = (body.len() as u32).to_be_bytes().to_vec();
        f.extend_from_slice(body);
        f
    }

    #[tokio::test(start_paused = true)]
    async fn slow_frames_survive_the_ticker() {
        let (mut tx, rx) = duplex(1024);
        tokio::spawn(async move {
            let first = frame(b"12345678");
            tx.write_all(&first[..5]).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            tx.write_all(&first[5..]).await.unwrap();
            let second = frame(b"second");
            tx.write_all(&second[..2]).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            tx.write_all(&second[2..]).await.unwrap();
        });
        let (frames, ticks) = tokio::time::timeout(Duration::from_secs(5), collect(rx, Duration::from_millis(30), 2))
            .await
            .expect("collect завис: после отмены чтения поток съехал, и кадр ждёт байтов, которые уже потеряны")
            .expect("collect вернул ошибку: после отмены чтения поток съехал и кадры разобрались неверно");
        assert_eq!(frames, vec![b"12345678".to_vec(), b"second".to_vec()], "кадры, пришедшие частями, должны собраться целыми");
        assert!(ticks >= 4, "таймер должен тикать, пока ждём данные (тиков {ticks})");
    }

    #[tokio::test(start_paused = true)]
    async fn clean_end_is_not_an_error() {
        let (mut tx, rx) = duplex(1024);
        tokio::spawn(async move {
            tx.write_all(&frame(b"only")).await.unwrap();
        });
        let (frames, _) = collect(rx, Duration::from_millis(30), 5).await.unwrap();
        assert_eq!(frames, vec![b"only".to_vec()], "конец потока после целого кадра штатный");
    }

    #[tokio::test(start_paused = true)]
    async fn truncated_frame_at_the_end_is_an_error() {
        let (mut tx, rx) = duplex(1024);
        tokio::spawn(async move {
            tx.write_all(&frame(b"abcdef")[..6]).await.unwrap();
        });
        let err = collect(rx, Duration::from_millis(30), 5).await.expect_err("поток оборвался посреди кадра");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "ждали UnexpectedEof, получили {err}");
    }
}
