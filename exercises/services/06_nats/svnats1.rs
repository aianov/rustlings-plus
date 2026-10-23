// Брокер присылает клиенту не готовые сообщения, а поток байтов: строки управления кончаются
// на \r\n, а тело сообщения идёт сразу за такой строкой, и его длина названа в ней. Тело может
// содержать любые байты (в том числе \r\n) и может прийти кусками, на середине.
// Цель: parse_op берёт первую операцию из начала буфера. Не хватает байтов: Ok(None).
// Операция целая: Ok(Some((операция, сколько байт она заняла))). Это не NATS: Err.
// Форматы строк описаны в README главы.


fn main() {}

#[derive(Debug, PartialEq)]
pub enum Op {
    Info(String),
    Ping,
    Pong,
    Ok,
    Err(String),
    Msg { subject: String, sid: u64, reply: Option<String>, payload: Vec<u8> },
    HMsg { subject: String, sid: u64, reply: Option<String>, headers: Vec<u8>, payload: Vec<u8> },
}

#[derive(Debug, PartialEq)]
pub struct ProtoError(pub String);

pub const MAX_LINE: usize = 4096;
pub const MAX_BODY: usize = 8 * 1024 * 1024;

pub fn parse_op(buf: &[u8]) -> Result<Option<(Op, usize)>, ProtoError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> (Op, usize) {
        parse_op(bytes).expect("корректные байты не ошибка").expect("операция целая, а parse_op просит ещё байтов")
    }

    #[test]
    fn control_lines_without_a_body() {
        assert_eq!(one(b"PING\r\n"), (Op::Ping, 6), "PING");
        assert_eq!(one(b"PONG\r\n"), (Op::Pong, 6), "PONG");
        assert_eq!(one(b"+OK\r\n"), (Op::Ok, 5), "+OK");
        assert_eq!(
            one(b"-ERR 'Unknown Protocol Operation'\r\n"),
            (Op::Err("Unknown Protocol Operation".to_string()), 35),
            "текст ошибки приходит в одинарных кавычках, наружу они не нужны"
        );
        let info = b"INFO {\"server_id\":\"X\",\"max_payload\":1048576}\r\n";
        assert_eq!(
            one(info),
            (Op::Info("{\"server_id\":\"X\",\"max_payload\":1048576}".to_string()), info.len()),
            "INFO несёт JSON без перевода строки"
        );
    }

    #[test]
    fn msg_with_and_without_reply() {
        let plain = b"MSG social.post.liked.alice 9 5\r\nhello\r\n";
        assert_eq!(
            one(plain),
            (
                Op::Msg { subject: "social.post.liked.alice".to_string(), sid: 9, reply: None, payload: b"hello".to_vec() },
                plain.len()
            ),
            "MSG без reply-to"
        );
        let asked = b"MSG notify.unread.bob 2 _INBOX.abc 2\r\n42\r\n";
        assert_eq!(
            one(asked),
            (
                Op::Msg { subject: "notify.unread.bob".to_string(), sid: 2, reply: Some("_INBOX.abc".to_string()), payload: b"42".to_vec() },
                asked.len()
            ),
            "MSG с reply-to: четвёртое слово это длина, третье это адрес ответа"
        );
    }

    #[test]
    fn payload_is_taken_by_length_not_by_line_end() {
        let wire = b"MSG chat.room 1 12\r\nline1\r\nline2\r\n";
        let (op, used) = one(wire);
        assert_eq!(used, wire.len(), "съесть надо всё сообщение целиком");
        match op {
            Op::Msg { payload, .. } => assert_eq!(payload, b"line1\r\nline2", "\\r\\n внутри тела это данные, а не конец сообщения"),
            other => panic!("ожидали MSG, получили {other:?}"),
        }
        let empty = b"MSG a.b 1 0\r\n\r\n";
        assert_eq!(one(empty).1, empty.len(), "пустое тело: за строкой управления всё равно идёт \\r\\n");
        let binary = [b'M', b'S', b'G', b' ', b'x', b' ', b'1', b' ', b'4', b'\r', b'\n', 0xff, 0x00, 0xfe, 0x80, b'\r', b'\n'];
        match one(&binary).0 {
            Op::Msg { payload, .. } => assert_eq!(payload, [0xff, 0x00, 0xfe, 0x80], "тело это байты, а не UTF-8 строка"),
            other => panic!("ожидали MSG, получили {other:?}"),
        }
    }

    #[test]
    fn hmsg_splits_headers_and_payload() {
        let wire = b"HMSG a.b 5 _INBOX.q 21 26\r\nNATS/1.0\r\nX-Id: 7\r\n\r\nhello\r\n";
        let (op, used) = one(wire);
        assert_eq!(used, wire.len(), "HMSG съедается целиком");
        assert_eq!(
            op,
            Op::HMsg {
                subject: "a.b".to_string(),
                sid: 5,
                reply: Some("_INBOX.q".to_string()),
                headers: b"NATS/1.0\r\nX-Id: 7\r\n\r\n".to_vec(),
                payload: b"hello".to_vec()
            },
            "первое число это длина заголовков, второе это длина заголовков вместе с телом"
        );
        let no_reply = b"HMSG a.b 5 12 12\r\nNATS/1.0\r\n\r\n\r\n";
        match one(no_reply).0 {
            Op::HMsg { reply, headers, payload, .. } => {
                assert_eq!((reply, headers.len(), payload.len()), (None, 12, 0), "без reply-to и без тела");
            }
            other => panic!("ожидали HMSG, получили {other:?}"),
        }
    }

    #[test]
    fn every_prefix_of_a_message_asks_for_more_bytes() {
        let wire: &[u8] = b"MSG chat.room 1 _INBOX.z 12\r\nab\r\ncd\r\nef\r\n\r\n";
        for cut in 0..wire.len() {
            assert_eq!(
                parse_op(&wire[..cut]),
                Ok(None),
                "первые {cut} байт из {} это ещё не сообщение: ждём продолжения, а не ошибка и не паника",
                wire.len()
            );
        }
        assert!(matches!(parse_op(wire), Ok(Some((Op::Msg { .. }, n))) if n == wire.len()), "а целое сообщение разбирается");
        let hwire: &[u8] = b"HMSG a.b 5 12 17\r\nNATS/1.0\r\n\r\nhello\r\n";
        for cut in 0..hwire.len() {
            assert_eq!(parse_op(&hwire[..cut]), Ok(None), "HMSG: первые {cut} байт это ещё не сообщение");
        }
    }

    #[test]
    fn several_operations_in_one_buffer_are_taken_one_by_one() {
        let wire: &[u8] = b"PING\r\nMSG a 1 3\r\nx\r\n\r\nMSG b 1 1\r\ny\r\n+OK\r\nPART";
        let mut rest = wire;
        let mut seen = Vec::new();
        while let Some((op, used)) = parse_op(rest).expect("разбор") {
            seen.push(op);
            rest = &rest[used..];
        }
        assert_eq!(seen.len(), 4, "PING, два MSG и +OK: {seen:?}");
        assert_eq!(rest, b"PART", "недоразобранный хвост остаётся в буфере нетронутым");
        assert_eq!(seen[1], Op::Msg { subject: "a".to_string(), sid: 1, reply: None, payload: b"x\r\n".to_vec() }, "тело второго сообщения");
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic_and_not_a_wait() {
        for bad in [
            &b"HELLO\r\n"[..],
            b"\r\n",
            b"MSG a 1 x\r\n",
            b"MSG a 1\r\n",
            b"MSG a b 3\r\nabc\r\n",
            b"MSG a 1 3\r\nabcde\r\n",
            b"HMSG a 1 9 5\r\n",
            b"MSG a 1 18446744073709551615\r\n",
            b"MSG a 1 99999999999999999999999\r\n",
        ] {
            assert!(parse_op(bad).is_err(), "{:?} не NATS, ждём Err сразу", String::from_utf8_lossy(bad));
        }
        let endless = vec![b'x'; 10_000];
        assert!(parse_op(&endless).is_err(), "10 000 байт без \\r\\n: ждать конца строки вечно нельзя, иначе буфер растёт без предела");
        let huge = format!("MSG a 1 {}\r\n", MAX_BODY + 1);
        assert!(parse_op(huge.as_bytes()).is_err(), "тело больше лимита отвергается по строке управления, не дожидаясь байтов");
    }
}
