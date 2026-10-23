use bytes::BytesMut;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    pub fn from_u8(value: u8) -> Option<Opcode> {
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

    pub fn to_u8(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }

    pub fn is_control(self) -> bool {
        self.to_u8() & 0x8 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
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
pub enum Role {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin: true, opcode, payload: payload.to_vec() }
    }

    pub fn text(s: &str) -> Frame {
        Frame::new(Opcode::Text, s.as_bytes())
    }

    pub fn part(fin: bool, opcode: Opcode, payload: &[u8]) -> Frame {
        Frame { fin, opcode, payload: payload.to_vec() }
    }
}

pub fn close_code(err: &ProtoError) -> u16 {
    todo!()
}

#[derive(Debug, Clone)]
pub struct Decoder {
    pub role: Role,
    pub max_payload: usize,
}

impl Decoder {
    pub fn new(role: Role, max_payload: usize) -> Decoder {
        Decoder { role, max_payload }
    }

    pub fn decode(&self, buf: &mut BytesMut) -> Result<Option<Frame>, ProtoError> {
        todo!()
    }
}

pub fn encode_frame(frame: &Frame, mask: Option<[u8; 4]>, out: &mut BytesMut) {
    todo!()
}

#[derive(Debug, Clone)]
pub struct MaskGen {
    state: u64,
}

impl MaskGen {
    pub fn with_seed(seed: u64) -> MaskGen {
        MaskGen { state: seed | 1 }
    }

    pub fn new() -> MaskGen {
        todo!()
    }

    pub fn next_key(&mut self) -> [u8; 4] {
        todo!()
    }
}

impl Default for MaskGen {
    fn default() -> MaskGen {
        MaskGen::new()
    }
}
