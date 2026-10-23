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

fn main() {
    for m in [Message::Ping, Message::Text("привет".into()), Message::Data(vec![1, 2, 3])] {
        let frame = encode_message(&m).unwrap();
        println!("{m:?} -> {frame:?}");
        assert_eq!(parse_message(frame[0], &frame[3..]), Ok(m));
    }
    println!("{:?}", parse_message(42, b""));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bytes() {
        assert_eq!(encode_message(&Message::Ping).unwrap(), [1, 0, 0], "Ping это тип 1 и пустое тело");
        assert_eq!(encode_message(&Message::Text("hi".into())).unwrap(), [2, 0, 2, b'h', b'i'], "Text это тип 2, длина 2 байта big-endian");
        assert_eq!(encode_message(&Message::Data(vec![9; 300])).unwrap()[..3], [3, 1, 44], "300 это 0x012C");
    }

    #[test]
    fn round_trip() {
        for m in [Message::Ping, Message::Text("привет, мир".into()), Message::Data(vec![]), Message::Data(vec![0, 255, 7])] {
            let frame = encode_message(&m).unwrap();
            assert_eq!(parse_message(frame[0], &frame[3..]), Ok(m.clone()), "туда и обратно должно совпасть: {m:?}");
        }
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        assert_eq!(parse_message(9, b""), Err(ProtoError::UnknownType(9)), "неизвестный тип");
        assert_eq!(parse_message(2, &[0xff, 0xfe]), Err(ProtoError::BadUtf8), "Text с битым UTF-8");
        assert_eq!(parse_message(1, b"x"), Err(ProtoError::BadBody), "у Ping тело обязано быть пустым");
    }

    #[test]
    fn body_too_long_for_u16() {
        assert_eq!(encode_message(&Message::Data(vec![0; 65535])).unwrap().len(), 3 + 65535, "65535 ещё помещается");
        assert_eq!(encode_message(&Message::Data(vec![0; 65536])), Err(ProtoError::TooLong), "65536 не помещается в u16");
    }
}
