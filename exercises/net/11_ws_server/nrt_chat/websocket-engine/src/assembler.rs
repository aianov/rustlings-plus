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
