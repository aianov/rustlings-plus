// Первое, что делает сервер с сокетом после апгрейда, это читает фреймы. Сеть отдаёт байты как придётся: половину
// заголовка, два фрейма сразу, фрейм и ещё кусочек следующего. Декодер должен жить с этим: когда данных мало, молча
// ждать, когда много, выдавать по одному фрейму, а лишнее оставлять в буфере.
// TODO: Допиши Decoder::decode. Мелкие функции (два байта, длина, маска) уже готовы. Если фрейма пока нет целиком,
// верни `Ok(None)` и не трогай буфер. Слишком большой фрейм надо отвергнуть по заявленной длине, не дожидаясь данных.

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
        let _ = buf;
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(fin: bool, opcode: u8, payload: &[u8], key: Option<[u8; 4]>) -> Vec<u8> {
        let mut out = vec![(u8::from(fin) << 7) | opcode];
        let mask_bit = if key.is_some() { 0x80 } else { 0 };
        match payload.len() {
            n if n < 126 => out.push(mask_bit | n as u8),
            n if n < 65536 => {
                out.push(mask_bit | 126);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                out.push(mask_bit | 127);
                out.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        match key {
            Some(key) => {
                out.extend_from_slice(&key);
                out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
            }
            None => out.extend_from_slice(payload),
        }
        out
    }

    const KEY: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];

    #[test]
    fn unmasked_frame_for_the_client_role() {
        let d = Decoder::new(Role::Client, 1 << 20);
        let mut buf = BytesMut::from(&b"\x81\x05Hello"[..]);
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::text("Hello")));
        assert!(buf.is_empty(), "фрейм прочитан целиком, из буфера он должен исчезнуть");
    }

    #[test]
    fn masked_rfc_example_for_the_server_role() {
        let d = Decoder::new(Role::Server, 1 << 20);
        let mut buf = BytesMut::from(&[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58][..]);
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::text("Hello")));
    }

    #[test]
    fn wrong_mask_for_the_role_is_an_error() {
        let server = Decoder::new(Role::Server, 1 << 20);
        let mut plain = BytesMut::from(&wire(true, 1, b"x", None)[..]);
        assert_eq!(server.decode(&mut plain), Err(ProtoError::MaskRequired), "клиент прислал фрейм без маски");
        let client = Decoder::new(Role::Client, 1 << 20);
        let mut masked = BytesMut::from(&wire(true, 1, b"x", Some(KEY))[..]);
        assert_eq!(client.decode(&mut masked), Err(ProtoError::MaskForbidden), "сервер прислал фрейм с маской");
    }

    #[test]
    fn partial_data_waits_and_buffer_stays_intact() {
        let full = wire(true, 2, &[7u8; 300], Some(KEY));
        let d = Decoder::new(Role::Server, 1 << 20);
        let mut buf = BytesMut::new();
        for (i, byte) in full.iter().enumerate() {
            buf.extend_from_slice(&[*byte]);
            let before = buf.len();
            let got = d.decode(&mut buf).unwrap();
            if i + 1 < full.len() {
                assert!(got.is_none(), "после {} байт из {} фрейма ещё нет", i + 1, full.len());
                assert_eq!(buf.len(), before, "пока фрейма нет, декодер не имеет права забирать байты из буфера: докачка продолжит с того же места");
            } else {
                let frame = got.expect("фрейм должен появиться на последнем байте");
                assert_eq!(frame, Frame::new(Opcode::Binary, &[7u8; 300]));
                assert!(buf.is_empty());
            }
        }
    }

    #[test]
    fn two_frames_in_one_buffer_and_a_tail() {
        let mut all = wire(true, 1, b"one", Some(KEY));
        all.extend(wire(false, 2, b"two", Some([1, 2, 3, 4])));
        let third = wire(true, 9, b"ping", Some([9, 9, 9, 9]));
        all.extend_from_slice(&third[..3]);
        let d = Decoder::new(Role::Server, 1 << 20);
        let mut buf = BytesMut::from(&all[..]);
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::text("one")));
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::part(false, Opcode::Binary, b"two")));
        assert_eq!(d.decode(&mut buf).unwrap(), None, "третий фрейм пришёл не целиком");
        assert_eq!(buf.len(), 3, "его начало остаётся в буфере");
        buf.extend_from_slice(&third[3..]);
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::new(Opcode::Ping, b"ping")));
    }

    #[test]
    fn long_lengths() {
        let d = Decoder::new(Role::Server, 1 << 20);
        for size in [125usize, 126, 127, 65535, 65536, 70000] {
            let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let mut buf = BytesMut::from(&wire(true, 2, &payload, Some(KEY))[..]);
            let frame = d.decode(&mut buf).unwrap().unwrap_or_else(|| panic!("фрейм на {size} байт не распознан"));
            assert_eq!(frame.payload, payload, "полезная нагрузка {size} байт");
        }
    }

    #[test]
    fn too_big_is_rejected_before_the_payload_arrives() {
        let d = Decoder::new(Role::Server, 1000);
        let mut header = vec![0x82, 0xff];
        header.extend_from_slice(&(1u64 << 30).to_be_bytes());
        header.extend_from_slice(&KEY);
        let mut buf = BytesMut::from(&header[..]);
        assert_eq!(d.decode(&mut buf), Err(ProtoError::TooBig), "заявлен гигабайт: отказ должен прийти по заголовку, а не после ожидания гигабайта");
        let mut medium = BytesMut::from(&[0x82, 0xfe, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00][..]);
        assert_eq!(d.decode(&mut medium), Err(ProtoError::TooBig), "1024 байта при лимите 1000");
        let ok = wire(true, 2, &[1u8; 1000], Some(KEY));
        assert!(d.decode(&mut BytesMut::from(&ok[..])).unwrap().is_some(), "ровно по лимиту ещё можно");
    }

    #[test]
    fn header_errors_surface() {
        let d = Decoder::new(Role::Server, 1 << 20);
        let mut rsv = BytesMut::from(&[0xc1, 0x80, 0, 0, 0, 0][..]);
        assert_eq!(d.decode(&mut rsv), Err(ProtoError::ReservedBits));
        let mut bad = BytesMut::from(&[0x83, 0x80, 0, 0, 0, 0][..]);
        assert_eq!(d.decode(&mut bad), Err(ProtoError::BadOpcode(3)));
    }

    #[test]
    fn empty_payload() {
        let d = Decoder::new(Role::Server, 1 << 20);
        let mut buf = BytesMut::from(&wire(true, 9, b"", Some(KEY))[..]);
        assert_eq!(d.decode(&mut buf).unwrap(), Some(Frame::new(Opcode::Ping, b"")));
    }
}
