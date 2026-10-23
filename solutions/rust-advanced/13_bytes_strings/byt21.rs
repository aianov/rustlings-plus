#[derive(Debug, PartialEq)]
pub enum ParseError {
    Incomplete { needed: usize },
    Invalid(&'static str),
}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub kind: u8,
    pub text: String,
}

pub fn parse_message(buf: &[u8]) -> Result<(Message, usize), ParseError> {
    let Some(&kind) = buf.first() else {
        return Err(ParseError::Incomplete { needed: 3 });
    };
    if kind != 1 && kind != 2 {
        return Err(ParseError::Invalid("unknown kind"));
    }
    let Some(len_bytes) = buf.get(1..3) else {
        return Err(ParseError::Incomplete { needed: 3 - buf.len() });
    };
    let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
    if len > 1000 {
        return Err(ParseError::Invalid("too long"));
    }
    let Some(body) = buf.get(3..3 + len) else {
        return Err(ParseError::Incomplete { needed: 3 + len - buf.len() });
    };
    let text = std::str::from_utf8(body).map_err(|_| ParseError::Invalid("bad utf-8"))?;
    Ok((Message { kind, text: text.to_string() }, 3 + len))
}

fn main() {
    println!("{:?}", parse_message(&[1, 0, 2, b'h', b'i']));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(kind: u8, text: &str) -> Vec<u8> {
        let mut v = vec![kind];
        v.extend_from_slice(&(text.len() as u16).to_be_bytes());
        v.extend_from_slice(text.as_bytes());
        v
    }

    #[test]
    fn a_complete_frame_is_parsed_with_its_size() {
        let mut buf = frame(1, "привет");
        let size = buf.len();
        buf.extend_from_slice(b"next frame bytes");
        assert_eq!(parse_message(&buf), Ok((Message { kind: 1, text: "привет".to_string() }, size)), "хвост после кадра не мешает и не входит в размер");
    }

    #[test]
    fn every_prefix_of_a_frame_asks_for_more_bytes() {
        let buf = frame(2, "ok 👍");
        for cut in 0..buf.len() {
            let expected = if cut < 3 { 3 - cut } else { buf.len() - cut };
            assert_eq!(parse_message(&buf[..cut]), Err(ParseError::Incomplete { needed: expected }), "кадр обрезан после {cut} байт");
        }
        assert!(parse_message(&buf).is_ok());
    }

    #[test]
    fn garbage_is_rejected_as_soon_as_it_is_visible() {
        assert_eq!(parse_message(&[9]), Err(ParseError::Invalid("unknown kind")), "вид известен по первому байту, остальное ждать не надо");
        assert_eq!(parse_message(&[0, 0, 1, b'x']), Err(ParseError::Invalid("unknown kind")));
        assert_eq!(parse_message(&[1, 0x27, 0x10]), Err(ParseError::Invalid("too long")), "длина 10000 известна по заголовку: ждать 10000 байт не нужно");
        assert_eq!(parse_message(&[1, 0x03, 0xe9]), Err(ParseError::Invalid("too long")), "1001 больше предела");
    }

    #[test]
    fn complete_but_broken_text_is_invalid_not_incomplete() {
        let buf = [1, 0, 3, 0xff, 0xfe, 0xfd];
        assert_eq!(parse_message(&buf), Err(ParseError::Invalid("bad utf-8")), "все три байта на месте, и это не текст: ждать нечего");
        let cut = [1, 0, 2, 0xd0, 0xd0];
        assert_eq!(parse_message(&cut), Err(ParseError::Invalid("bad utf-8")));
    }

    #[test]
    fn the_limit_itself_is_allowed() {
        let text = "a".repeat(1000);
        let buf = frame(1, &text);
        assert_eq!(parse_message(&buf).map(|(m, n)| (m.text.len(), n)), Ok((1000, 1003)));
    }
}
