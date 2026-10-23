use crate::frame::{Frame, Opcode, ProtoError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Open,
    Closing,
    Closed,
}

pub fn valid_close_code(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999)
}

pub fn parse_close(payload: &[u8]) -> Result<Option<(u16, String)>, ProtoError> {
    match payload.len() {
        0 => Ok(None),
        1 => Err(ProtoError::BadClose),
        _ => {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            if !valid_close_code(code) {
                return Err(ProtoError::BadClose);
            }
            let reason = std::str::from_utf8(&payload[2..]).map_err(|_| ProtoError::InvalidUtf8)?;
            Ok(Some((code, reason.to_string())))
        }
    }
}

pub fn encode_close(code: u16, reason: &str) -> Vec<u8> {
    let mut end = reason.len().min(123);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = code.to_be_bytes().to_vec();
    out.extend_from_slice(&reason.as_bytes()[..end]);
    out
}

#[derive(Debug)]
pub struct Closing {
    phase: Phase,
}

impl Default for Closing {
    fn default() -> Closing {
        Closing::new()
    }
}

impl Closing {
    pub fn new() -> Closing {
        Closing { phase: Phase::Open }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn accepts_data(&self) -> bool {
        self.phase == Phase::Open
    }

    pub fn start(&mut self, code: u16, reason: &str) -> Option<Frame> {
        if self.phase != Phase::Open {
            return None;
        }
        self.phase = Phase::Closing;
        Some(Frame::new(Opcode::Close, &encode_close(code, reason)))
    }

    pub fn on_close(&mut self, payload: &[u8]) -> Option<Frame> {
        match self.phase {
            Phase::Open => {
                self.phase = Phase::Closed;
                let reply = match parse_close(payload) {
                    Ok(None) => Vec::new(),
                    Ok(Some((code, _))) => encode_close(code, ""),
                    Err(ProtoError::InvalidUtf8) => encode_close(1007, ""),
                    Err(_) => encode_close(1002, ""),
                };
                Some(Frame::new(Opcode::Close, &reply))
            }
            Phase::Closing => {
                self.phase = Phase::Closed;
                None
            }
            Phase::Closed => None,
        }
    }

    pub fn mark_closed(&mut self) {
        self.phase = Phase::Closed;
    }
}
