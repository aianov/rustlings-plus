// Длина полезной нагрузки хранится в нескольких форматах, чтобы маленькие сообщения не платили за большие: до 125
// байт прямо в семи битах, дальше семь бит содержат маркер 126 (и длина идёт в следующих двух байтах) или 127 (в
// следующих восьми). Данные приходят по сети кусками, поэтому следующих байтов может ещё не быть.
// TODO: Допиши parse_len: по семи битам и остатку буфера верни длину и число прочитанных байтов, либо `None`, если
// байтов не хватает. Запрещены: старший бит 64-битной длины и неминимальная запись (126 с числом меньше 126 и т. д.).

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

fn parse_len(len7: u8, rest: &[u8]) -> Result<Option<(u64, usize)>, ProtoError> {
    match len7 {
        0..=125 => Ok(Some((u64::from(len7), 0))),
        126 => {
            if rest.len() < 2 {
                return Ok(None);
            }
            let n = u64::from(u16::from_be_bytes([rest[0], rest[1]]));
            if n < 126 {
                return Err(ProtoError::NonMinimalLength);
            }
            Ok(Some((n, 2)))
        }
        _ => {
            if rest.len() < 8 {
                return Ok(None);
            }
            let n = u64::from_be_bytes(rest[..8].try_into().expect("ровно 8 байт"));
            if n >> 63 != 0 {
                return Err(ProtoError::LengthHighBit);
            }
            if n < 65536 {
                return Err(ProtoError::NonMinimalLength);
            }
            Ok(Some((n, 8)))
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_lengths_live_in_the_first_bytes() {
        for n in 0..=125u8 {
            assert_eq!(parse_len(n, &[]), Ok(Some((u64::from(n), 0))), "длина {n} помещается в семь бит, лишних байтов читать не надо");
            assert_eq!(parse_len(n, &[1, 2, 3]), Ok(Some((u64::from(n), 0))), "чужие байты после заголовка не трогаем");
        }
    }

    #[test]
    fn sixteen_bit_lengths() {
        assert_eq!(parse_len(126, &[0x00, 0x7e]), Ok(Some((126, 2))));
        assert_eq!(parse_len(126, &[0x01, 0x00]), Ok(Some((256, 2))));
        assert_eq!(parse_len(126, &[0xff, 0xff, 0xaa]), Ok(Some((65535, 2))));
    }

    #[test]
    fn sixty_four_bit_lengths() {
        assert_eq!(parse_len(127, &[0, 0, 0, 0, 0, 1, 0, 0]), Ok(Some((65536, 8))));
        assert_eq!(parse_len(127, &[0, 0, 0, 0, 1, 0, 0, 0]), Ok(Some((1 << 24, 8))));
        assert_eq!(parse_len(127, &[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), Ok(Some((i64::MAX as u64, 8))));
    }

    #[test]
    fn not_enough_bytes_means_wait() {
        assert_eq!(parse_len(126, &[]), Ok(None));
        assert_eq!(parse_len(126, &[0x01]), Ok(None), "половина двухбайтной длины");
        for have in 0..8 {
            assert_eq!(parse_len(127, &vec![0u8; have]), Ok(None), "для восьмибайтной длины есть только {have} байт");
        }
    }

    #[test]
    fn the_high_bit_is_forbidden() {
        assert_eq!(parse_len(127, &[0x80, 0, 0, 0, 0, 0, 0, 0]), Err(ProtoError::LengthHighBit), "старший бит 64-битной длины обязан быть нулём");
        assert_eq!(parse_len(127, &[0xff; 8]), Err(ProtoError::LengthHighBit));
    }

    #[test]
    fn shortest_form_only() {
        assert_eq!(parse_len(126, &[0x00, 0x7d]), Err(ProtoError::NonMinimalLength), "125 надо было записать в семи битах");
        assert_eq!(parse_len(126, &[0x00, 0x00]), Err(ProtoError::NonMinimalLength));
        assert_eq!(parse_len(127, &[0, 0, 0, 0, 0, 0, 0xff, 0xff]), Err(ProtoError::NonMinimalLength), "65535 влезает в 16 бит");
        assert_eq!(parse_len(127, &[0, 0, 0, 0, 0, 0, 0, 5]), Err(ProtoError::NonMinimalLength));
    }
}
