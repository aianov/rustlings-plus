// Сервер, который собирает сообщения без предела, это сервер, которого можно уронить: клиент шлёт фрагменты по
// килобайту, никогда не ставя FIN, и память растёт, пока процесс не умрёт. Проверять надо не размер одного фрейма
// (он мал), а размер всего сообщения. А когда предел превышен или протокол нарушен, соединение закрывают с кодом,
// который объясняет причину.
// TODO: Научи Assembler лимиту max_message (он считает сумму по всем фрагментам и срабатывает раньше, чем лишнее
// скопится) и допиши close_code: какой код закрытия отправить на каждую ошибку.

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
    max_message: usize,
}

impl Assembler {
    fn new(max_message: usize) -> Assembler {
        Assembler { partial: None, max_message }
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
        if part.bytes.len() + part.tail.len() + chunk.len() > self.max_message {
            return Err(ProtoError::TooBig);
        }
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

fn close_code(err: &ProtoError) -> u16 {
    match err {
        ProtoError::InvalidUtf8 => 1007,
        ProtoError::TooBig => 1009,
        _ => 1002,
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Event {
        Event::Message(Message::Text(s.to_string()))
    }

    #[test]
    fn a_message_exactly_at_the_limit_is_fine() {
        let mut a = Assembler::new(10);
        assert_eq!(a.push(Frame::new(Opcode::Binary, &[0u8; 10])).unwrap(), Some(Event::Message(Message::Binary(vec![0u8; 10]))));
        let mut a = Assembler::new(10);
        a.push(Frame::part(false, Opcode::Text, b"12345")).unwrap();
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"67890")).unwrap(), Some(text("1234567890")));
    }

    #[test]
    fn one_frame_over_the_limit() {
        let mut a = Assembler::new(10);
        assert_eq!(a.push(Frame::new(Opcode::Binary, &[0u8; 11])), Err(ProtoError::TooBig));
        let mut a = Assembler::new(10);
        assert_eq!(a.push(Frame::text("12345678901")), Err(ProtoError::TooBig), "и для текста тоже");
    }

    #[test]
    fn the_sum_of_small_fragments_is_checked_as_they_come() {
        let mut a = Assembler::new(450);
        let mut failed_at = None;
        for i in 0..10 {
            let opcode = if i == 0 { Opcode::Binary } else { Opcode::Continuation };
            match a.push(Frame::part(false, opcode, &[0u8; 100])) {
                Ok(None) => {}
                Err(ProtoError::TooBig) => {
                    failed_at = Some(i);
                    break;
                }
                other => panic!("фрагмент {i}: неожиданный результат {other:?}"),
            }
        }
        assert_eq!(failed_at, Some(4), "пятый фрагмент доводит сумму до 500 при лимите 450: отказывать нужно сразу на нём, а не копить дальше и не ждать FIN");
    }

    #[test]
    fn limit_applies_to_text_with_a_pending_tail_too() {
        let mut a = Assembler::new(4);
        a.push(Frame::part(false, Opcode::Text, &[b'a', b'b', 0xf0, 0x9f])).unwrap();
        assert_eq!(a.push(Frame::part(false, Opcode::Continuation, &[0x98])), Err(ProtoError::TooBig), "недописанный хвост символа тоже считается в размер");
    }

    #[test]
    fn control_frames_do_not_count() {
        let mut a = Assembler::new(10);
        a.push(Frame::part(false, Opcode::Binary, &[0u8; 8])).unwrap();
        for _ in 0..20 {
            assert!(matches!(a.push(Frame::new(Opcode::Ping, &[0u8; 100])), Ok(Some(Event::Control(_)))), "ping посреди сообщения не считается его частью");
        }
        assert!(a.push(Frame::part(true, Opcode::Continuation, &[0u8; 2])).unwrap().is_some());
    }

    #[test]
    fn the_other_rules_still_work() {
        let mut a = Assembler::new(100);
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"x")), Err(ProtoError::UnexpectedContinuation));
        let mut a = Assembler::new(100);
        assert_eq!(a.push(Frame::new(Opcode::Text, &[0xff])), Err(ProtoError::InvalidUtf8));
    }

    #[test]
    fn close_codes() {
        assert_eq!(close_code(&ProtoError::TooBig), 1009, "сообщение слишком велико");
        assert_eq!(close_code(&ProtoError::InvalidUtf8), 1007, "данные не соответствуют типу сообщения");
        for err in [
            ProtoError::ReservedBits,
            ProtoError::BadOpcode(3),
            ProtoError::NonMinimalLength,
            ProtoError::LengthHighBit,
            ProtoError::MaskRequired,
            ProtoError::MaskForbidden,
            ProtoError::FragmentedControl,
            ProtoError::ControlTooLong,
            ProtoError::UnexpectedContinuation,
            ProtoError::ExpectedContinuation,
            ProtoError::BadClose,
        ] {
            assert_eq!(close_code(&err), 1002, "{err:?} это нарушение протокола, код 1002");
        }
    }
}
