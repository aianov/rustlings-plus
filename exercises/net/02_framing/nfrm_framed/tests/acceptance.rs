use framing::{Decoder, Encoder, Framed, LengthCodec, LengthError, LinesCodec, LinesError};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt, duplex};
use tokio::time::{sleep, timeout};

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(Duration::from_secs(5), fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: не уложились в 5 секунд, ожидание зависло"),
    }
}

#[tokio::test]
async fn lines_round_trip_both_ways() {
    let (a, b) = duplex(1024);
    let mut left = Framed::new(a, LinesCodec::new(64));
    let mut right = Framed::new(b, LinesCodec::new(64));
    left.send("hello".to_owned()).await.unwrap();
    right.send("world".to_owned()).await.unwrap();
    assert_eq!(within("recv справа", right.recv()).await.unwrap().as_deref(), Some("hello"), "строка должна дойти без изменений");
    assert_eq!(within("recv слева", left.recv()).await.unwrap().as_deref(), Some("world"));
}

#[tokio::test]
async fn length_round_trip() {
    let (a, b) = duplex(1024);
    let mut left = Framed::new(a, LengthCodec::new(1024));
    let mut right = Framed::new(b, LengthCodec::new(1024));
    left.send(b"abc".to_vec()).await.unwrap();
    left.send(vec![]).await.unwrap();
    left.send(vec![7; 500]).await.unwrap();
    assert_eq!(within("1", right.recv()).await.unwrap(), Some(b"abc".to_vec()));
    assert_eq!(within("2", right.recv()).await.unwrap(), Some(vec![]), "пустой кадр это тоже кадр");
    assert_eq!(within("3", right.recv()).await.unwrap(), Some(vec![7; 500]));
}

#[tokio::test]
async fn glued_frames_come_out_separately() {
    let (mut raw, b) = duplex(1024);
    let mut framed = Framed::new(b, LinesCodec::new(64));
    raw.write_all(b"one\ntwo\nthree\nfo").await.unwrap();
    for want in ["one", "two", "three"] {
        assert_eq!(within("recv", framed.recv()).await.unwrap().as_deref(), Some(want), "склеенные в одном чтении сообщения должны выниматься по одному");
    }
    raw.write_all(b"ur\n").await.unwrap();
    assert_eq!(within("recv", framed.recv()).await.unwrap().as_deref(), Some("four"), "хвост должен дождаться продолжения");
}

#[tokio::test]
async fn byte_by_byte_delivery() {
    let (mut raw, b) = duplex(16);
    let mut framed = Framed::new(b, LengthCodec::new(64));
    let writer = tokio::spawn(async move {
        for byte in [0u8, 0, 0, 3, b'x', b'y', b'z', 0, 0, 0, 1, b'!'] {
            raw.write_all(&[byte]).await.unwrap();
            sleep(Duration::from_millis(2)).await;
        }
    });
    assert_eq!(within("1", framed.recv()).await.unwrap(), Some(b"xyz".to_vec()));
    assert_eq!(within("2", framed.recv()).await.unwrap(), Some(b"!".to_vec()));
    writer.await.unwrap();
    assert_eq!(within("EOF", framed.recv()).await.unwrap(), None, "писатель закрылся между кадрами: чистый конец");
}

#[tokio::test]
async fn clean_end_and_truncated_end() {
    let (raw, b) = duplex(64);
    let mut framed = Framed::new(b, LinesCodec::new(64));
    drop(raw);
    assert_eq!(within("EOF", framed.recv()).await.unwrap(), None, "чистый конец это Ok(None)");
    assert_eq!(within("EOF ещё раз", framed.recv()).await.unwrap(), None, "после конца потока recv продолжает отвечать Ok(None)");

    let (mut raw, b) = duplex(64);
    let mut framed = Framed::new(b, LengthCodec::new(64));
    raw.write_all(&[0, 0, 0, 5, 1, 2]).await.unwrap();
    drop(raw);
    match within("обрыв", framed.recv()).await {
        Err(LengthError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof, "обрыв посреди кадра должен быть UnexpectedEof"),
        other => panic!("ждали Err(Io(UnexpectedEof)), получили {other:?}"),
    }
}

#[tokio::test]
async fn limits_protect_memory() {
    let (mut raw, b) = duplex(64);
    let mut framed = Framed::new(b, LengthCodec::new(1024));
    raw.write_all(&[0x40, 0, 0, 0, 1]).await.unwrap();
    assert!(matches!(within("лимит", framed.recv()).await, Err(LengthError::TooLong)), "заявлен 1 ГБ при лимите 1 КБ: отказ по заголовку");

    let (mut raw, b) = duplex(4096);
    let mut framed = Framed::new(b, LinesCodec::new(16));
    raw.write_all(&[b'a'; 200]).await.unwrap();
    assert!(matches!(within("лимит", framed.recv()).await, Err(LinesError::TooLong)), "200 байт без '\\n' при лимите 16: ждать конца строки нельзя");

    let (a, _b) = duplex(64);
    let mut framed = Framed::new(a, LinesCodec::new(3));
    assert!(matches!(framed.send("toolong".to_owned()).await, Err(LinesError::TooLong)), "отправка длиннее лимита это ошибка");
}

#[tokio::test]
async fn recv_is_cancel_safe() {
    let (mut raw, b) = duplex(64);
    let mut framed = Framed::new(b, LengthCodec::new(64));
    raw.write_all(&[0, 0, 0, 6, b'a', b'b']).await.unwrap();
    for _ in 0..5 {
        assert!(
            timeout(Duration::from_millis(20), framed.recv()).await.is_err(),
            "кадр ещё не весь, recv должен ждать, а не возвращать мусор"
        );
    }
    raw.write_all(b"cdef").await.unwrap();
    assert_eq!(
        within("recv", framed.recv()).await.unwrap(),
        Some(b"abcdef".to_vec()),
        "recv отменяли по таймауту, пока кадр приходил частями: ни байта потеряться не должно, буфер живёт в Framed, а не в вызове"
    );
}

#[derive(Default)]
struct Recorder {
    writes: Vec<Vec<u8>>,
}

impl AsyncWrite for Recorder {
    fn poll_write(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.writes.push(buf.to_vec());
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn send_is_one_write_per_message() {
    let mut framed = Framed::new(Recorder::default(), LengthCodec::new(64));
    framed.send(b"hello".to_vec()).await.unwrap();
    framed.send(b"hi".to_vec()).await.unwrap();
    let (rec, _, _) = framed.into_parts();
    assert_eq!(rec.writes.len(), 2, "каждое сообщение должно уходить одной записью (заголовок и тело вместе), а было {} записей", rec.writes.len());
    assert_eq!(rec.writes[0], [0, 0, 0, 5, b'h', b'e', b'l', b'l', b'o'], "формат кадра: u32 big-endian и тело");
}

#[tokio::test]
async fn into_parts_returns_unread_bytes() {
    let (mut raw, b) = duplex(64);
    let mut framed = Framed::new(b, LinesCodec::new(64));
    raw.write_all(b"first\nsecond\nthi").await.unwrap();
    assert_eq!(within("recv", framed.recv()).await.unwrap().as_deref(), Some("first"));
    let (_stream, _codec, rest) = framed.into_parts();
    assert_eq!(&rest[..], b"second\nthi", "прочитанное сверх кадра принадлежит тому, кто заберёт поток дальше (апгрейд протокола), оно не должно пропасть");
}

#[test]
fn codecs_work_without_any_io() {
    let mut codec = LinesCodec::new(8);
    let mut buf = bytes::BytesMut::new();
    codec.encode("ping".to_owned(), &mut buf).unwrap();
    assert_eq!(&buf[..], b"ping\n");
    assert_eq!(codec.decode(&mut buf).unwrap().as_deref(), Some("ping"));
    let mut buf = bytes::BytesMut::from(&b"\xff\n"[..]);
    assert!(matches!(codec.decode(&mut buf), Err(LinesError::BadUtf8)), "не UTF-8");
}
