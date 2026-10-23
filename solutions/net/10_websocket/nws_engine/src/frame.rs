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
    match err {
        ProtoError::InvalidUtf8 => 1007,
        ProtoError::TooBig => 1009,
        _ => 1002,
    }
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
        if buf.len() < 2 {
            return Ok(None);
        }
        let b0 = buf[0];
        let b1 = buf[1];
        if b0 & 0x70 != 0 {
            return Err(ProtoError::ReservedBits);
        }
        let opcode = Opcode::from_u8(b0 & 0x0F).ok_or(ProtoError::BadOpcode(b0 & 0x0F))?;
        let masked = b1 & 0x80 != 0;
        match (self.role, masked) {
            (Role::Server, false) => return Err(ProtoError::MaskRequired),
            (Role::Client, true) => return Err(ProtoError::MaskForbidden),
            _ => {}
        }
        let len7 = b1 & 0x7F;
        let (len, len_bytes) = match len7 {
            0..=125 => (u64::from(len7), 0),
            126 => {
                if buf.len() < 4 {
                    return Ok(None);
                }
                let n = u64::from(u16::from_be_bytes([buf[2], buf[3]]));
                if n < 126 {
                    return Err(ProtoError::NonMinimalLength);
                }
                (n, 2)
            }
            _ => {
                if buf.len() < 10 {
                    return Ok(None);
                }
                let n = u64::from_be_bytes(buf[2..10].try_into().expect("ровно 8 байт"));
                if n >> 63 != 0 {
                    return Err(ProtoError::LengthHighBit);
                }
                if n < 65536 {
                    return Err(ProtoError::NonMinimalLength);
                }
                (n, 8)
            }
        };
        if opcode.is_control() {
            if b0 & 0x80 == 0 {
                return Err(ProtoError::FragmentedControl);
            }
            if len > 125 {
                return Err(ProtoError::ControlTooLong);
            }
        }
        if len > self.max_payload as u64 {
            return Err(ProtoError::TooBig);
        }
        let len = len as usize;
        let key_bytes = if masked { 4 } else { 0 };
        let total = 2 + len_bytes + key_bytes + len;
        if buf.len() < total {
            return Ok(None);
        }
        let key_at = 2 + len_bytes;
        let mut payload = buf[key_at + key_bytes..total].to_vec();
        if masked {
            let key = [buf[key_at], buf[key_at + 1], buf[key_at + 2], buf[key_at + 3]];
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= key[i % 4];
            }
        }
        let _ = buf.split_to(total);
        Ok(Some(Frame { fin: b0 & 0x80 != 0, opcode, payload }))
    }
}

pub fn encode_frame(frame: &Frame, mask: Option<[u8; 4]>, out: &mut BytesMut) {
    out.extend_from_slice(&[(u8::from(frame.fin) << 7) | frame.opcode.to_u8()]);
    let mask_bit = if mask.is_some() { 0x80 } else { 0 };
    let len = frame.payload.len();
    if len < 126 {
        out.extend_from_slice(&[mask_bit | len as u8]);
    } else if len < 65536 {
        out.extend_from_slice(&[mask_bit | 126]);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.extend_from_slice(&[mask_bit | 127]);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            out.extend(frame.payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        }
        None => out.extend_from_slice(&frame.payload),
    }
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
        use std::hash::{BuildHasher, Hasher};
        let seed = std::collections::hash_map::RandomState::new().build_hasher().finish();
        MaskGen::with_seed(seed)
    }

    pub fn next_key(&mut self) -> [u8; 4] {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let mixed = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        ((mixed >> 32) as u32).to_be_bytes()
    }
}

impl Default for MaskGen {
    fn default() -> MaskGen {
        MaskGen::new()
    }
}
