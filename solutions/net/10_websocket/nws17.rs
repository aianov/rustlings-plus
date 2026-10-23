// Чат с эмодзи: пользователь шлёт длинное сообщение, а клиентская библиотека режет его на фрагменты по 4 байта,
// и в какой-то момент граница попадает внутрь смайлика. Сервер закрывает соединение с кодом 1007 (плохой UTF-8),
// хотя текст безупречен. Другой тест из набора Autobahn требует обратного: если уже в первом фрагменте видно, что
// это не UTF-8, соединение надо закрыть сразу, не дожидаясь последнего фрагмента, который может не прийти вовсе.
// TODO: Почини Assembler: корректный текст, разрезанный где угодно, должен собираться, а испорченный должен вызывать
// InvalidUtf8 на самом первом фрагменте, где это видно. Остальное поведение сборщика не меняй.

#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    fn from_u8(value: u8) -> Option<Opcode> {
        match value {
            0x0 => Some(Opcode::Continuation),
            0x1 => Some(Opcode::Text),
            0x2 => Some(Opcode::Binary),
            0x8 => Some(Opcode::Close),
            0x9 => Some(Opcode::Ping),
            0xA => Some(Opcode::Pong),
            _ => None,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }

    fn is_control(self) -> bool {
        self.to_u8() & 0x8 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProtoError {
    ReservedBits,
    BadOpcode(u8),
    NonMinimalLength,
    LengthHighBit,
    MaskRequired,
    MaskForbidden,
    TooBig,
    FragmentedControl,
    ControlTooLong,
    UnexpectedContinuation,
    ExpectedContinuation,
    InvalidUtf8,
    BadClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    fin: bool,
    opcode: Opcode,
    payload: Vec<u8>,
}

impl Frame {
    fn new(opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin: true, opcode, payload: payload.to_vec() }
    }

    fn text(s: &str) -> Frame {
        Frame::new(Opcode::Text, s.as_bytes())
    }

    fn part(fin: bool, opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin, opcode, payload: payload.to_vec() }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Message {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Message(Message),
    Control(Frame),
}

struct Partial {
    opcode: Opcode,
    bytes: Vec<u8>,
    tail: Vec<u8>,
}

struct Assembler {
    partial: Option<Partial>,
}

impl Assembler {
    fn new() -> Assembler {
        Assembler { partial: None }
    }

    fn push(&mut self, frame: Frame) -> Result<Option<Event>, ProtoError> {
        match frame.opcode {
            Opcode::Ping | Opcode::Pong | Opcode::Close => {
                if !frame.fin {
                    return Err(ProtoError::FragmentedControl);
                }
                if frame.payload.len() > 125 {
                    return Err(ProtoError::ControlTooLong);
                }
                Ok(Some(Event::Control(frame)))
            }
            Opcode::Text | Opcode::Binary => {
                if self.partial.is_some() {
                    return Err(ProtoError::ExpectedContinuation);
                }
                let part = Partial { opcode: frame.opcode, bytes: Vec::new(), tail: Vec::new() };
                self.absorb(part, &frame.payload, frame.fin)
            }
            Opcode::Continuation => {
                let Some(part) = self.partial.take() else {
                    return Err(ProtoError::UnexpectedContinuation);
                };
                self.absorb(part, &frame.payload, frame.fin)
            }
        }
    }

    fn absorb(&mut self, mut part: Partial, chunk: &[u8], fin: bool) -> Result<Option<Event>, ProtoError> {
        if part.opcode == Opcode::Text {
            let mut pending = std::mem::take(&mut part.tail);
            pending.extend_from_slice(chunk);
            match std::str::from_utf8(&pending) {
                Ok(_) => part.bytes.extend_from_slice(&pending),
                Err(e) if e.error_len().is_some() => return Err(ProtoError::InvalidUtf8),
                Err(e) => {
                    let good = e.valid_up_to();
                    part.bytes.extend_from_slice(&pending[..good]);
                    part.tail = pending[good..].to_vec();
                }
            }
            if fin && !part.tail.is_empty() {
                return Err(ProtoError::InvalidUtf8);
            }
        } else {
            part.bytes.extend_from_slice(chunk);
        }

        if !fin {
            self.partial = Some(part);
            return Ok(None);
        }
        let message = match part.opcode {
            Opcode::Text => Message::Text(String::from_utf8(part.bytes).map_err(|_| ProtoError::InvalidUtf8)?),
            _ => Message::Binary(part.bytes),
        };
        Ok(Some(Event::Message(message)))
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(parts: &[(&[u8], bool)]) -> Vec<Result<Option<Event>, ProtoError>> {
        let mut a = Assembler::new();
        let mut out = Vec::new();
        for (i, (bytes, fin)) in parts.iter().enumerate() {
            let opcode = if i == 0 { Opcode::Text } else { Opcode::Continuation };
            out.push(a.push(Frame::part(*fin, opcode, bytes)));
        }
        out
    }

    fn text(s: &str) -> Event {
        Event::Message(Message::Text(s.to_string()))
    }

    #[test]
    fn emoji_cut_into_single_bytes() {
        let bytes = "a😀b".as_bytes();
        let parts: Vec<(&[u8], bool)> = bytes.chunks(1).enumerate().map(|(i, c)| (c, i + 1 == bytes.len())).collect();
        let results = feed(&parts);
        assert_eq!(results.last().unwrap(), &Ok(Some(text("a😀b"))), "эмодзи в четыре байта, разрезанное по байтам, это корректный текст");
        for r in &results[..results.len() - 1] {
            assert_eq!(r, &Ok(None), "пока сообщение не закончилось, готового сообщения нет и ошибки тоже");
        }
    }

    #[test]
    fn every_cut_of_valid_text_is_fine() {
        let whole = "Привет, мир! 日本語 😀 end";
        let bytes = whole.as_bytes();
        for first in 0..=bytes.len() {
            for second in first..=bytes.len() {
                let parts = [(&bytes[..first], false), (&bytes[first..second], false), (&bytes[second..], true)];
                let results = feed(&parts);
                assert_eq!(results.last().unwrap(), &Ok(Some(text(whole))), "разрез в позициях {first} и {second} испортил хороший текст");
            }
        }
    }

    #[test]
    fn garbage_fails_on_the_first_bad_fragment() {
        let r = feed(&[(&[0x61, 0xff], false), (b"never", true)]);
        assert_eq!(r[0], Err(ProtoError::InvalidUtf8), "0xff не бывает в UTF-8: ждать последнего фрагмента не нужно");
        let r = feed(&[(&[0xc0, 0x80], false), (b"x", true)]);
        assert_eq!(r[0], Err(ProtoError::InvalidUtf8), "overlong-запись нуля");
        let r = feed(&[(&[0xed, 0xa0, 0x80], false), (b"x", true)]);
        assert_eq!(r[0], Err(ProtoError::InvalidUtf8), "суррогатная половина");
    }

    #[test]
    fn garbage_in_the_middle_of_a_split_character() {
        let r = feed(&[(&[0xe2], false), (&[0x28, 0xa1], false), (b"x", true)]);
        assert_eq!(r[0], Ok(None), "начало трёхбайтного символа пока допустимо");
        assert_eq!(r[1], Err(ProtoError::InvalidUtf8), "после 0xe2 не может идти 0x28: ошибка на этом фрагменте, не позже");
    }

    #[test]
    fn incomplete_character_at_the_end_is_an_error() {
        let r = feed(&[(&[b'a', 0xf0, 0x9f], true)]);
        assert_eq!(r[0], Err(ProtoError::InvalidUtf8), "последний фрагмент оборвался на полуслове");
        let r = feed(&[(&[0xf0, 0x9f], false), (&[0x98], true)]);
        assert_eq!(r[1], Err(ProtoError::InvalidUtf8), "символ так и не закончился");
    }

    #[test]
    fn binary_is_never_validated() {
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Binary, &[0xff, 0xc0])).unwrap();
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, &[0xed, 0xa0])).unwrap(), Some(Event::Message(Message::Binary(vec![0xff, 0xc0, 0xed, 0xa0]))));
    }

    #[test]
    fn the_rest_of_the_assembler_is_unchanged() {
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Text, b"He")).unwrap();
        let ping = Frame::new(Opcode::Ping, b"p");
        assert_eq!(a.push(ping.clone()).unwrap(), Some(Event::Control(ping)));
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"llo")).unwrap(), Some(text("Hello")));
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"x")), Err(ProtoError::UnexpectedContinuation));
    }
}
