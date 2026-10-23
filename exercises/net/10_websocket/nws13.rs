// Теперь в обратную сторону: из Frame собрать байты. Сервер пишет фреймы как есть, клиент обязан замаскировать их
// ключом, который каждый раз новый и непредсказуемый. Длину надо записать кратчайшим способом, иначе чужой
// декодер (такой, как ты написал в прошлых задачах) справедливо откажет.
// TODO: Допиши encode_frame и MaskGen. Случайных чисел из внешнего крейта у нас нет: хватит xorshift, но зерно
// возьми из `RandomState`, который std инициализирует случайно, чтобы разные процессы не давали одинаковые ключи.

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
    let _ = (frame, mask, out);
    todo!()
}

struct MaskGen {
    state: u64,
}

impl MaskGen {
    fn with_seed(seed: u64) -> MaskGen {
        MaskGen { state: seed | 1 }
    }

    fn new() -> MaskGen {
        todo!()
    }

    fn next_key(&mut self) -> [u8; 4] {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(frame: &Frame, mask: Option<[u8; 4]>) -> Vec<u8> {
        let mut out = BytesMut::new();
        encode_frame(frame, mask, &mut out);
        out.to_vec()
    }

    #[test]
    fn server_text_frame() {
        assert_eq!(encode(&Frame::text("Hello"), None), b"\x81\x05Hello");
    }

    #[test]
    fn client_masked_frame_from_rfc() {
        assert_eq!(
            encode(&Frame::text("Hello"), Some([0x37, 0xfa, 0x21, 0x3d])),
            vec![0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58],
            "пример из RFC 6455: заголовок, ключ, замаскированные данные"
        );
    }

    #[test]
    fn flags_and_opcodes() {
        assert_eq!(encode(&Frame::part(false, Opcode::Text, b"a"), None)[0], 0x01, "первый фрагмент: FIN=0, opcode=1");
        assert_eq!(encode(&Frame::part(true, Opcode::Continuation, b"a"), None)[0], 0x80);
        assert_eq!(encode(&Frame::new(Opcode::Ping, b""), None), vec![0x89, 0x00]);
        assert_eq!(encode(&Frame::new(Opcode::Close, &[0x03, 0xe8]), None), vec![0x88, 0x02, 0x03, 0xe8]);
    }

    #[test]
    fn length_forms() {
        let h = |n: usize| encode(&Frame::new(Opcode::Binary, &vec![0u8; n]), None);
        assert_eq!(&h(125)[..2], &[0x82, 125]);
        assert_eq!(&h(126)[..4], &[0x82, 126, 0x00, 126], "126 уже не влезает в семь бит");
        assert_eq!(&h(65535)[..4], &[0x82, 126, 0xff, 0xff]);
        assert_eq!(&h(65536)[..10], &[0x82, 127, 0, 0, 0, 0, 0, 1, 0, 0], "65536 это уже восемь байт");
        assert_eq!(h(65536).len(), 10 + 65536);
    }

    #[test]
    fn mask_bit_and_length_together() {
        let bytes = encode(&Frame::new(Opcode::Binary, &[0u8; 300]), Some([1, 2, 3, 4]));
        assert_eq!(bytes[1], 0x80 | 126, "бит маски стоит в том же байте, что и маркер длины");
        assert_eq!(&bytes[2..4], &[0x01, 0x2c]);
        assert_eq!(&bytes[4..8], &[1, 2, 3, 4], "ключ идёт после длины");
        assert_eq!(bytes.len(), 8 + 300);
    }

    #[test]
    fn our_decoder_reads_what_our_encoder_writes() {
        let mut keys = MaskGen::with_seed(5);
        for size in [0usize, 1, 5, 125, 126, 1000, 65535, 65536, 100000] {
            let payload: Vec<u8> = (0..size).map(|i| (i * 7 % 256) as u8).collect();
            for opcode in [Opcode::Text, Opcode::Binary] {
                let frame = Frame { fin: size % 2 == 0, opcode, payload: payload.clone() };
                let masked = encode(&frame, Some(keys.next_key()));
                let server = Decoder::new(Role::Server, 1 << 20);
                assert_eq!(server.decode(&mut BytesMut::from(&masked[..])).unwrap(), Some(frame.clone()), "клиентский фрейм из {size} байт");
                let plain = encode(&frame, None);
                let client = Decoder::new(Role::Client, 1 << 20);
                assert_eq!(client.decode(&mut BytesMut::from(&plain[..])).unwrap(), Some(frame), "серверный фрейм из {size} байт");
            }
        }
    }

    #[test]
    fn encode_appends_to_the_buffer() {
        let mut out = BytesMut::from(&b"xx"[..]);
        encode_frame(&Frame::text("a"), None, &mut out);
        assert_eq!(&out[..], b"xx\x81\x01a", "кодировщик дописывает в конец буфера, а не затирает его");
    }

    #[test]
    fn masking_changes_the_payload() {
        let frame = Frame::new(Opcode::Binary, &[0u8; 8]);
        let bytes = encode(&frame, Some([0xaa, 0xbb, 0xcc, 0xdd]));
        assert_ne!(&bytes[6..], &[0u8; 8], "данные на проводе должны быть замаскированы");
    }

    #[test]
    fn mask_keys_are_not_a_constant() {
        let mut gen_a = MaskGen::new();
        let keys: std::collections::HashSet<[u8; 4]> = (0..1000).map(|_| gen_a.next_key()).collect();
        assert!(keys.len() > 990, "из 1000 ключей различных только {}: ключ маски должен меняться на каждый фрейм", keys.len());
    }

    #[test]
    fn different_generators_have_different_seeds() {
        let mut a = MaskGen::new();
        let mut b = MaskGen::new();
        let first_a: Vec<[u8; 4]> = (0..4).map(|_| a.next_key()).collect();
        let first_b: Vec<[u8; 4]> = (0..4).map(|_| b.next_key()).collect();
        assert_ne!(first_a, first_b, "два генератора с одинаковым зерном дадут одинаковые ключи: зерно должно быть случайным");
    }

    #[test]
    fn seeded_generator_is_repeatable() {
        let mut a = MaskGen::with_seed(42);
        let mut b = MaskGen::with_seed(42);
        let mut c = MaskGen::with_seed(44);
        let (ka, kb, kc) = (a.next_key(), b.next_key(), c.next_key());
        assert_eq!(ka, kb);
        assert_ne!(ka, kc);
    }
}
