use std::future::Future;
use std::io::ErrorKind;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use framing::{Decoder, Encoder, Framed, LengthCodec, LengthError, LinesCodec, LinesError};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn trickle(pieces: Vec<Vec<u8>>) -> DuplexStream {
    let (mut tx, rx) = duplex(64);
    tokio::spawn(async move {
        for piece in pieces {
            if tx.write_all(&piece).await.is_err() {
                return;
            }
            tokio::task::yield_now().await;
        }
    });
    rx
}

fn pieces(parts: &[&str]) -> Vec<Vec<u8>> {
    parts.iter().map(|part| part.as_bytes().to_vec()).collect()
}

#[test]
fn decoder_is_a_pure_function_over_the_buffer() {
    let mut codec = LinesCodec::new(16);
    let mut buf = BytesMut::from("раз\nдва\r\nтри");
    assert_eq!(
        codec
            .decode(&mut buf)
            .expect("строка разбирается")
            .as_deref(),
        Some("раз"),
        "первая строка без \\n"
    );
    assert_eq!(
        codec
            .decode(&mut buf)
            .expect("строка разбирается")
            .as_deref(),
        Some("два"),
        "\\r перед \\n тоже срезается"
    );
    assert_eq!(
        codec
            .decode(&mut buf)
            .expect("без \\n не ошибка")
            .as_deref(),
        None,
        "без \\n строка не готова"
    );
    assert_eq!(
        &buf[..],
        "три".as_bytes(),
        "остаток ждёт в буфере, а не теряется"
    );
}

#[tokio::test]
async fn lines_survive_any_split_of_the_stream() {
    let stream = trickle(pieces(&["при", "вет\nм", "ир\n", "\nпос", "ледняя\n"]));
    let mut frames = Framed::new(stream, LinesCodec::new(64));
    let mut lines = Vec::new();
    while let Some(line) = within("строка", frames.recv())
        .await
        .expect("строки читаются")
    {
        lines.push(line);
    }
    assert_eq!(
        lines,
        ["привет", "мир", "", "последняя"],
        "склейка и разрыв не важны: строка заканчивается на \\n, многобайтные буквы режутся где угодно"
    );
}

#[tokio::test]
async fn eof_in_the_middle_of_a_line_is_an_error() {
    let stream = trickle(pieces(&["целая\n", "оборван"]));
    let mut frames = Framed::new(stream, LinesCodec::new(64));
    assert_eq!(
        within("строка", frames.recv())
            .await
            .expect("первая строка целая")
            .as_deref(),
        Some("целая")
    );
    let torn = within("обрыв", frames.recv()).await;
    assert!(
        matches!(&torn, Err(LinesError::Io(error)) if error.kind() == ErrorKind::UnexpectedEof),
        "поток кончился посреди строки: это UnexpectedEof, а не тихая потеря и не пустая строка; пришло {torn:?}"
    );
}

#[tokio::test]
async fn long_line_is_refused_without_waiting_for_its_end() {
    let (mut tx, rx) = duplex(1 << 16);
    tx.write_all(&[b'x'; 200]).await.expect("байты пишутся");
    let mut frames = Framed::new(rx, LinesCodec::new(100));
    let refused = within("длинная строка без конца", frames.recv()).await;
    assert!(
        matches!(refused, Err(LinesError::TooLong { max: 100 })),
        "строка уже длиннее лимита, а \\n всё нет: ошибка сразу, без ожидания и без роста буфера; пришло {refused:?}"
    );
    let mut codec = LinesCodec::new(3);
    let mut buf = BytesMut::from("четыре\n");
    assert!(
        matches!(codec.decode(&mut buf), Err(LinesError::TooLong { max: 3 })),
        "целая строка длиннее лимита тоже отказ"
    );
}

#[test]
fn broken_utf8_is_reported_and_skipped() {
    let mut codec = LinesCodec::new(64);
    let mut buf = BytesMut::from(&b"\xff\xfe\nok\n"[..]);
    assert!(
        matches!(codec.decode(&mut buf), Err(LinesError::BadUtf8)),
        "не UTF-8 это BadUtf8"
    );
    assert_eq!(
        codec
            .decode(&mut buf)
            .expect("следующая строка целая")
            .as_deref(),
        Some("ok"),
        "испорченная строка вынута из буфера, следующая читается"
    );
}

#[tokio::test]
async fn send_writes_one_line_per_item() {
    let (tx, mut rx) = duplex(1 << 16);
    let mut frames = Framed::new(tx, LinesCodec::new(64));
    within("запись", frames.send("первая"))
        .await
        .expect("строка пишется");
    within("запись", frames.send(String::from("вторая")))
        .await
        .expect("строка пишется");
    let too_long = within("запись", frames.send("x".repeat(65))).await;
    assert!(
        matches!(too_long, Err(LinesError::TooLong { max: 64 })),
        "отправить строку длиннее лимита нельзя: другая сторона её всё равно не примет"
    );
    within("закрытие", frames.close())
        .await
        .expect("запись закрывается");
    let mut written = String::new();
    within("чтение", rx.read_to_string(&mut written))
        .await
        .expect("записанное читается");
    assert_eq!(written, "первая\nвторая\n", "каждый send это строка и \\n");
}

#[tokio::test]
async fn length_frames_survive_any_split() {
    let mut wire = BytesMut::new();
    let mut codec = LengthCodec::new(1024);
    for frame in [&b"hello"[..], &b""[..], &[7u8; 300][..]] {
        codec.encode(frame, &mut wire).expect("кадр кодируется");
    }
    let bytes = wire.to_vec();
    let parts: Vec<Vec<u8>> = bytes.chunks(3).map(<[u8]>::to_vec).collect();
    let mut frames = Framed::new(trickle(parts), LengthCodec::new(1024));
    let mut got = Vec::new();
    while let Some(frame) = within("кадр", frames.recv()).await.expect("кадры читаются")
    {
        got.push(frame);
    }
    assert_eq!(
        got,
        [
            Bytes::from_static(b"hello"),
            Bytes::new(),
            Bytes::from(vec![7u8; 300])
        ],
        "длина впереди (u32, big-endian) и тело: кадры, нарезанные по три байта, собираются обратно, пустой кадр тоже кадр"
    );
}

#[tokio::test]
async fn huge_length_is_refused_before_reading_the_body() {
    let (mut tx, rx) = duplex(64);
    tx.write_all(&u32::MAX.to_be_bytes())
        .await
        .expect("заголовок пишется");
    let mut frames = Framed::new(rx, LengthCodec::new(1 << 20));
    let refused = within("кадр на 4 ГиБ", frames.recv()).await;
    assert!(
        matches!(
            refused,
            Err(LengthError::TooLong {
                len: 4_294_967_295,
                max: 1_048_576
            })
        ),
        "длина больше лимита это отказ сразу по заголовку, до того как выделять память и ждать тело; пришло {refused:?}"
    );
    let mut codec = LengthCodec::new(4);
    let mut wire = BytesMut::new();
    assert!(
        matches!(
            codec.encode(&b"12345"[..], &mut wire),
            Err(LengthError::TooLong { len: 5, max: 4 })
        ),
        "кодировать кадр длиннее лимита тоже нельзя"
    );
}

#[tokio::test]
async fn eof_in_the_middle_of_a_frame_is_an_error() {
    let (mut tx, rx) = duplex(64);
    tx.write_all(&[0, 0, 0, 10, 1, 2, 3])
        .await
        .expect("полкадра пишется");
    drop(tx);
    let mut frames = Framed::new(rx, LengthCodec::new(64));
    let torn = within("обрыв кадра", frames.recv()).await;
    assert!(
        matches!(&torn, Err(LengthError::Io(error)) if error.kind() == ErrorKind::UnexpectedEof),
        "обещали 10 байт, пришло 3 и конец: это UnexpectedEof; пришло {torn:?}"
    );
}
