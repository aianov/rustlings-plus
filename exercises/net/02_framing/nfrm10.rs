// Тот же формат кадра, что в прошлой задаче (encode_message и parse_message уже есть). Теперь нужен
// декодер для сокета: ему дают буфер, в котором могут лежать полтора кадра, три кадра или
// два байта заголовка. Он не читает сокет и не ждёт: это чистая функция над буфером.
// TODO: Допиши decode. Целый кадр вырезается из буфера и разбирается. Если кадр ещё не весь
// пришёл, верни Ok(None) и не трогай буфер: дочитывать будет вызывающий код. Тело длиннее MAX_BODY
// это TooLong, и сообщать об этом нужно как только известна длина, не дожидаясь самого тела.

use bytes::BytesMut;

#[derive(Debug, PartialEq, Clone)]
enum Message {
    Ping,
    Text(String),
    Data(Vec<u8>),
}

#[derive(Debug, PartialEq)]
enum ProtoError {
    UnknownType(u8),
    BadUtf8,
    BadBody,
    TooLong,
}

fn encode_message(message: &Message) -> Result<Vec<u8>, ProtoError> {
    let (kind, body): (u8, &[u8]) = match message {
        Message::Ping => (1, &[]),
        Message::Text(text) => (2, text.as_bytes()),
        Message::Data(bytes) => (3, bytes),
    };
    let len = u16::try_from(body.len()).map_err(|_| ProtoError::TooLong)?;
    let mut frame = Vec::with_capacity(3 + body.len());
    frame.push(kind);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

fn parse_message(kind: u8, body: &[u8]) -> Result<Message, ProtoError> {
    match kind {
        1 if body.is_empty() => Ok(Message::Ping),
        1 => Err(ProtoError::BadBody),
        2 => String::from_utf8(body.to_vec()).map(Message::Text).map_err(|_| ProtoError::BadUtf8),
        3 => Ok(Message::Data(body.to_vec())),
        other => Err(ProtoError::UnknownType(other)),
    }
}

const MAX_BODY: usize = 1024;

fn decode(buf: &mut BytesMut) -> Result<Option<Message>, ProtoError> {
    todo!()
}

fn main() {
    let mut buf = BytesMut::new();
    for m in [Message::Ping, Message::Text("hi".into())] {
        buf.extend_from_slice(&encode_message(&m).unwrap());
    }
    while let Ok(Some(m)) = decode(&mut buf) {
        println!("{m:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(messages: &[Message]) -> Vec<u8> {
        messages.iter().flat_map(|m| encode_message(m).unwrap()).collect()
    }

    fn sample() -> Vec<Message> {
        vec![Message::Text("hello".into()), Message::Ping, Message::Data(vec![1, 2, 3]), Message::Text("мир".into())]
    }

    #[test]
    fn several_frames_in_one_buffer() {
        let mut buf = BytesMut::from(&wire(&sample())[..]);
        for want in sample() {
            assert_eq!(decode(&mut buf).unwrap(), Some(want), "кадры должны выниматься по одному, по порядку");
        }
        assert_eq!(decode(&mut buf).unwrap(), None, "буфер пуст");
        assert!(buf.is_empty());
    }

    #[test]
    fn partial_frame_leaves_the_buffer_untouched() {
        let bytes = wire(&[Message::Text("hello".into())]);
        for cut in 0..bytes.len() {
            let mut buf = BytesMut::from(&bytes[..cut]);
            assert_eq!(decode(&mut buf).unwrap(), None, "кадр пришёл на {cut} из {} байт: ждём остальное", bytes.len());
            assert_eq!(buf.len(), cut, "при неполном кадре буфер трогать нельзя: из него уже нельзя будет собрать кадр, когда придёт остаток");
        }
    }

    #[test]
    fn byte_by_byte_feeding() {
        let bytes = wire(&sample());
        let mut buf = BytesMut::new();
        let mut got = Vec::new();
        for b in bytes {
            buf.extend_from_slice(&[b]);
            while let Some(m) = decode(&mut buf).unwrap() {
                got.push(m);
            }
        }
        assert_eq!(got, sample(), "по одному байту результат должен быть тем же");
    }

    #[test]
    fn complete_frame_plus_start_of_next() {
        let mut bytes = wire(&[Message::Ping]);
        bytes.extend_from_slice(&[2, 0, 5, b'h']);
        let mut buf = BytesMut::from(&bytes[..]);
        assert_eq!(decode(&mut buf).unwrap(), Some(Message::Ping));
        assert_eq!(decode(&mut buf).unwrap(), None, "второй кадр ещё не весь");
        assert_eq!(&buf[..], [2, 0, 5, b'h'], "начало второго кадра должно остаться нетронутым");
    }

    #[test]
    fn errors() {
        let mut buf = BytesMut::from(&[9u8, 0, 0][..]);
        assert_eq!(decode(&mut buf), Err(ProtoError::UnknownType(9)), "неизвестный тип");
        let mut buf = BytesMut::from(&[2u8, 0x08, 0x00][..]);
        assert_eq!(decode(&mut buf), Err(ProtoError::TooLong), "заявлено 2048 байт при лимите 1024: отказ по заголовку, тело ждать не нужно");
    }
}
