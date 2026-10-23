// Кодек, который прошёл три теста на строке «Hello», на проде падает на фрейме ровно в 125 байт, или когда сеть
// разрезала поток внутри ключа маски, или когда два фрейма пришли одним куском. Вручную такие случаи не придумать,
// их находит случайный перебор. Свойство здесь простое: что закодировали, то и должны прочитать, как бы поток ни
// порезала сеть.
// TODO: Напиши check: прогони cases случайных наборов фреймов через любой кодек (трейт Codec) и верни Err с описанием,
// если хоть где-то прочитано не то, что отправлено. Проверка должна пропускать правильный кодек и ловить кодеки
// с одной тонкой поломкой (какие именно, тебе не скажут).

#![allow(dead_code)]
use bytes::BytesMut;

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

fn apply_mask(data: &mut [u8], key: [u8; 4], offset: usize) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= key[(offset % 4 + i) % 4];
    }
}

fn check_mask(role: Role, masked: bool) -> Result<(), ProtoError> {
    match (role, masked) {
        (Role::Server, false) => Err(ProtoError::MaskRequired),
        (Role::Client, true) => Err(ProtoError::MaskForbidden),
        _ => Ok(()),
    }
}

struct Decoder {
    role: Role,
    max_payload: usize,
}

impl Decoder {
    fn new(role: Role, max_payload: usize) -> Decoder {
        Decoder { role, max_payload }
    }

    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Frame>, ProtoError> {
        if buf.len() < 2 {
            return Ok(None);
        }
        let head = parse_first_two(buf[0], buf[1])?;
        check_mask(self.role, head.masked)?;
        let Some((len, len_bytes)) = parse_len(head.len7, &buf[2..])? else {
            return Ok(None);
        };
        if len > self.max_payload as u64 {
            return Err(ProtoError::TooBig);
        }
        let len = len as usize;
        let key_bytes = if head.masked { 4 } else { 0 };
        let total = 2 + len_bytes + key_bytes + len;
        if buf.len() < total {
            return Ok(None);
        }
        let key_at = 2 + len_bytes;
        let mut payload = buf[key_at + key_bytes..total].to_vec();
        if head.masked {
            let key = [buf[key_at], buf[key_at + 1], buf[key_at + 2], buf[key_at + 3]];
            apply_mask(&mut payload, key, 0);
        }
        let _ = buf.split_to(total);
        Ok(Some(Frame { fin: head.fin, opcode: head.opcode, payload }))
    }
}

fn encode_frame(frame: &Frame, mask: Option<[u8; 4]>, out: &mut BytesMut) {
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
            let mut payload = frame.payload.clone();
            apply_mask(&mut payload, key, 0);
            out.extend_from_slice(&payload);
        }
        None => out.extend_from_slice(&frame.payload),
    }
}

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u64() as u8).collect()
    }
}

trait Codec {
    fn encode(&self, frame: &Frame, key: [u8; 4]) -> Vec<u8>;
    fn decode(&self, chunks: &[&[u8]]) -> Result<Vec<Frame>, String>;
}

fn check(codec: &dyn Codec, seed: u64, cases: usize) -> Result<(), String> {
    let _ = (codec, seed, cases);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Fault {
        None,
        Boundary125,
        KeySplit,
        OneFramePerChunk,
        LongLength,
        FinLost,
        EmptyLost,
    }

    struct Mutant(Fault);

    fn try_one(fault: Fault, buf: &[u8]) -> Result<Option<(Frame, usize)>, String> {
        if buf.len() < 2 {
            return Ok(None);
        }
        let fin_bit = buf[0] & 0x80 != 0;
        let opcode = Opcode::from_u8(buf[0] & 0x0f).ok_or("неизвестный opcode")?;
        let len7 = buf[1] & 0x7f;
        let (len, mut pos) = if len7 == 126 || (len7 == 125 && fault == Fault::Boundary125) {
            if buf.len() < 4 {
                return Ok(None);
            }
            (usize::from(u16::from_be_bytes([buf[2], buf[3]])), 4)
        } else if len7 == 127 {
            if buf.len() < 10 {
                return Ok(None);
            }
            let n = u64::from_be_bytes(buf[2..10].try_into().unwrap());
            (if fault == Fault::LongLength { (n & 0xffff) as usize } else { n as usize }, 10)
        } else {
            (usize::from(len7), 2)
        };
        if fault == Fault::KeySplit && buf.len() > pos && buf.len() < pos + 4 {
            return Err("поток разрезан внутри ключа маски".to_string());
        }
        if buf.len() < pos + 4 {
            return Ok(None);
        }
        let key = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        pos += 4;
        if buf.len() < pos + len {
            return Ok(None);
        }
        let payload: Vec<u8> = buf[pos..pos + len].iter().enumerate().map(|(i, b)| b ^ key[i % 4]).collect();
        let fin = fin_bit || (fault == Fault::FinLost && opcode == Opcode::Continuation);
        Ok(Some((Frame { fin, opcode, payload }, pos + len)))
    }

    impl Codec for Mutant {
        fn encode(&self, frame: &Frame, key: [u8; 4]) -> Vec<u8> {
            let mut out = BytesMut::new();
            encode_frame(frame, Some(key), &mut out);
            out.to_vec()
        }

        fn decode(&self, chunks: &[&[u8]]) -> Result<Vec<Frame>, String> {
            let mut buf: Vec<u8> = Vec::new();
            let mut frames = Vec::new();
            for chunk in chunks {
                buf.extend_from_slice(chunk);
                while let Some((frame, used)) = try_one(self.0, &buf)? {
                    buf.drain(..used);
                    if !(self.0 == Fault::EmptyLost && frame.payload.is_empty()) {
                        frames.push(frame);
                    }
                    if self.0 == Fault::OneFramePerChunk {
                        break;
                    }
                }
            }
            Ok(frames)
        }
    }

    #[test]
    fn the_correct_codec_passes() {
        for seed in [1, 2, 3] {
            assert_eq!(check(&Mutant(Fault::None), seed, 150), Ok(()), "правильный кодек проверка пропускать обязана (зерно {seed})");
        }
    }

    #[test]
    fn every_subtle_fault_is_caught() {
        let notes = [
            (Fault::Boundary125, "фрейм ровно в 125 байт читается как длина в двух байтах: нужны граничные размеры"),
            (Fault::KeySplit, "декодер падает, когда поток разрезан внутри ключа маски: нужна нарезка по одному байту"),
            (Fault::OneFramePerChunk, "из куска с несколькими фреймами читается только первый: нужны несколько фреймов в одной порции"),
            (Fault::LongLength, "длина от 65536 теряет старшие байты: нужны большие фреймы"),
            (Fault::FinLost, "FIN=0 у продолжения читается как FIN=1: нужны фреймы Continuation с разными FIN"),
            (Fault::EmptyLost, "пустой фрейм теряется: нужна нулевая длина"),
        ];
        for (fault, note) in notes {
            for seed in [7, 12345] {
                let result = check(&Mutant(fault), seed, 300);
                assert!(result.is_err(), "поломка {fault:?} не поймана (зерно {seed}): {note}");
            }
        }
    }

    #[test]
    fn the_message_says_where_to_look() {
        let message = check(&Mutant(Fault::Boundary125), 7, 300).unwrap_err();
        assert!(message.len() > 20, "сообщение об ошибке должно говорить, что и на каком случае сломалось, а не просто «ошибка»: {message:?}");
    }
}
