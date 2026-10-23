use crate::frame::{Frame, Opcode, ProtoError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Open,
    Closing,
    Closed,
}

pub fn valid_close_code(code: u16) -> bool {
    todo!()
}

pub fn parse_close(payload: &[u8]) -> Result<Option<(u16, String)>, ProtoError> {
    todo!()
}

pub fn encode_close(code: u16, reason: &str) -> Vec<u8> {
    todo!()
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
        todo!()
    }

    pub fn on_close(&mut self, payload: &[u8]) -> Option<Frame> {
        todo!()
    }

    pub fn mark_closed(&mut self) {
        self.phase = Phase::Closed;
    }
}
