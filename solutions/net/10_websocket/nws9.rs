// Фрейм WebSocket начинается с двух байтов, и в них уложено почти всё: флаг последнего фрагмента, три
// зарезервированных бита, код операции, флаг маски и первые семь бит длины. Лишнего байта на каждое поле никто
// не тратит, поэтому читать их придётся битовыми масками.
// TODO: Допиши Opcode::from_u8 (значения из таблицы RFC, остальные неизвестны) и parse_first_two. Зарезервированные
// биты, которые мы не договаривались использовать, и неизвестный код операции это ошибки протокола.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Head2 {
    fin: bool,
    opcode: Opcode,
    masked: bool,
    len7: u8,
}

fn parse_first_two(b0: u8, b1: u8) -> Result<Head2, ProtoError> {
    if b0 & 0x70 != 0 {
        return Err(ProtoError::ReservedBits);
    }
    let opcode = Opcode::from_u8(b0 & 0x0F).ok_or(ProtoError::BadOpcode(b0 & 0x0F))?;
    Ok(Head2 { fin: b0 & 0x80 != 0, opcode, masked: b1 & 0x80 != 0, len7: b1 & 0x7F })
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_valid(nibble: u8) -> bool {
        matches!(nibble, 0 | 1 | 2 | 8 | 9 | 10)
    }

    #[test]
    fn text_frame_start_from_rfc() {
        let head = parse_first_two(0x81, 0x85).unwrap();
        assert_eq!(head, Head2 { fin: true, opcode: Opcode::Text, masked: true, len7: 5 }, "0x81 0x85 это финальный Text с маской и длиной 5");
    }

    #[test]
    fn fragment_without_fin() {
        let head = parse_first_two(0x01, 0x03).unwrap();
        assert!(!head.fin, "у первого фрагмента нет FIN");
        assert_eq!(head.opcode, Opcode::Text);
        assert!(!head.masked);
        assert_eq!(head.len7, 3);
        assert_eq!(parse_first_two(0x00, 0x00).unwrap().opcode, Opcode::Continuation);
    }

    #[test]
    fn opcodes_roundtrip_and_controls() {
        for op in [Opcode::Continuation, Opcode::Text, Opcode::Binary, Opcode::Close, Opcode::Ping, Opcode::Pong] {
            assert_eq!(Opcode::from_u8(op.to_u8()), Some(op));
        }
        assert!(Opcode::Ping.is_control() && Opcode::Pong.is_control() && Opcode::Close.is_control());
        assert!(!Opcode::Text.is_control() && !Opcode::Binary.is_control() && !Opcode::Continuation.is_control());
    }

    #[test]
    fn every_two_byte_combination() {
        for b0 in 0..=255u8 {
            for b1 in 0..=255u8 {
                let got = parse_first_two(b0, b1);
                if b0 & 0x70 != 0 {
                    assert_eq!(got, Err(ProtoError::ReservedBits), "байт {b0:#04x}: зарезервированные биты RSV1..3 должны быть нулями");
                } else if !reference_valid(b0 & 0x0F) {
                    assert_eq!(got, Err(ProtoError::BadOpcode(b0 & 0x0F)), "байт {b0:#04x}: код операции {} не определён", b0 & 0x0F);
                } else {
                    let head = got.unwrap_or_else(|e| panic!("байты {b0:#04x} {b1:#04x} корректны, а получили {e:?}"));
                    assert_eq!(head.fin, b0 >= 0x80, "FIN это старший бит первого байта");
                    assert_eq!(head.opcode.to_u8(), b0 & 0x0F);
                    assert_eq!(head.masked, b1 >= 0x80, "MASK это старший бит второго байта");
                    assert_eq!(head.len7, b1 & 0x7F);
                }
            }
        }
    }
}
