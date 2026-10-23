// Одно сообщение может приехать несколькими фреймами: первый несёт тип (Text или Binary) и FIN=0, следующие
// Continuation, последний FIN=1. Если сообщение большое или клиент не знает его длины заранее, это единственный способ.
// А посреди такой цепочки могут встать ping и pong. Сборщик получает фреймы по одному и отдаёт либо готовое
// сообщение, либо управляющий фрейм «как есть», либо «пока ничего».
// TODO: Допиши Assembler::push. Ошибки порядка: Continuation без начала, новое сообщение до конца прошлого, плохой
// UTF-8 у готового Text. Управляющие фреймы проходят насквозь и не прерывают сборку.

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

struct Assembler {
    partial: Option<(Opcode, Vec<u8>)>,
}

impl Assembler {
    fn new() -> Assembler {
        Assembler { partial: None }
    }

    fn push(&mut self, frame: Frame) -> Result<Option<Event>, ProtoError> {
        let _ = frame;
        todo!()
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
    fn single_frame_messages() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::text("привет")).unwrap(), Some(text("привет")));
        assert_eq!(a.push(Frame::new(Opcode::Binary, &[1, 2, 3])).unwrap(), Some(Event::Message(Message::Binary(vec![1, 2, 3]))));
        assert_eq!(a.push(Frame::text("")).unwrap(), Some(text("")), "пустое сообщение это тоже сообщение");
    }

    #[test]
    fn three_fragments_make_one_message() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::part(false, Opcode::Text, b"He")).unwrap(), None);
        assert_eq!(a.push(Frame::part(false, Opcode::Continuation, b"ll")).unwrap(), None);
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"o")).unwrap(), Some(text("Hello")));
        assert_eq!(a.push(Frame::text("next")).unwrap(), Some(text("next")), "после сборки можно начинать новое сообщение");
    }

    #[test]
    fn binary_fragments() {
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Binary, &[0xff, 0xfe])).unwrap();
        let done = a.push(Frame::part(true, Opcode::Continuation, &[0x00])).unwrap();
        assert_eq!(done, Some(Event::Message(Message::Binary(vec![0xff, 0xfe, 0x00]))), "в Binary любые байты");
    }

    #[test]
    fn ping_in_the_middle_does_not_break_the_message() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::part(false, Opcode::Text, b"He")).unwrap(), None);
        let ping = Frame::new(Opcode::Ping, b"p");
        assert_eq!(a.push(ping.clone()).unwrap(), Some(Event::Control(ping)), "ping посреди сообщения отдаётся сразу");
        let pong = Frame::new(Opcode::Pong, b"");
        assert_eq!(a.push(pong.clone()).unwrap(), Some(Event::Control(pong)));
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"llo")).unwrap(), Some(text("Hello")), "сборка не потеряна");
    }

    #[test]
    fn order_errors() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"x")), Err(ProtoError::UnexpectedContinuation), "продолжение без начала");
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Text, b"a")).unwrap();
        assert_eq!(a.push(Frame::text("new")), Err(ProtoError::ExpectedContinuation), "новое сообщение до конца старого");
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Binary, b"a")).unwrap();
        assert_eq!(a.push(Frame::new(Opcode::Binary, b"b")), Err(ProtoError::ExpectedContinuation));
    }

    #[test]
    fn stray_continuation_after_a_finished_message() {
        let mut a = Assembler::new();
        a.push(Frame::text("done")).unwrap();
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, b"x")), Err(ProtoError::UnexpectedContinuation));
    }

    #[test]
    fn control_frame_rules_apply_here_too() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::part(false, Opcode::Ping, b"")), Err(ProtoError::FragmentedControl));
        assert_eq!(a.push(Frame::new(Opcode::Pong, &[0u8; 126])), Err(ProtoError::ControlTooLong));
    }

    #[test]
    fn invalid_utf8_is_checked_on_the_whole_message() {
        let mut a = Assembler::new();
        assert_eq!(a.push(Frame::new(Opcode::Text, &[0xff, 0xfe])), Err(ProtoError::InvalidUtf8));
        let mut a = Assembler::new();
        a.push(Frame::part(false, Opcode::Text, &[0xd0])).unwrap();
        assert_eq!(a.push(Frame::part(true, Opcode::Continuation, &[0xbf])).unwrap(), Some(text("п")), "одна буква, разрезанная по байтам, это нормально");
    }
}
