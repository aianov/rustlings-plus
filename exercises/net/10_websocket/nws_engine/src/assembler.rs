use crate::frame::{Frame, Opcode, ProtoError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Message(Message),
    Control(Frame),
}

struct Partial {
    opcode: Opcode,
    bytes: Vec<u8>,
    tail: Vec<u8>,
}

pub struct Assembler {
    partial: Option<Partial>,
    max_message: usize,
}

impl Assembler {
    pub fn new(max_message: usize) -> Assembler {
        Assembler { partial: None, max_message }
    }

    pub fn push(&mut self, frame: Frame) -> Result<Option<Event>, ProtoError> {
        todo!()
    }

    fn absorb(&mut self, mut part: Partial, chunk: &[u8], fin: bool) -> Result<Option<Event>, ProtoError> {
        todo!()
    }
}
