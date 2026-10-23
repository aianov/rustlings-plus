// Клиент обязан маскировать каждый фрейм, который отправляет серверу: четыре случайных байта (ключ) повторяются по
// кругу и XOR-ятся с данными. Смысл не в секретности, ключ едет рядом, а в том, что чужой прокси не должен увидеть в
// потоке данных клиента что-то, похожее на HTTP-запрос. Сервер, наоборот, никогда не маскирует.
// TODO: Допиши apply_mask (она же снимает маску: XOR дважды возвращает исходное) и check_mask. Данные могут
// приходить кусками, поэтому apply_mask принимает смещение: номер байта данных, с которого начинается кусок.

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

fn apply_mask(data: &mut [u8], key: [u8; 4], offset: usize) {
    let _ = (data, key, offset);
    todo!()
}

fn check_mask(role: Role, masked: bool) -> Result<(), ProtoError> {
    let _ = (role, masked);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_example_hello() {
        let key = [0x37, 0xfa, 0x21, 0x3d];
        let mut data = [0x7f, 0x9f, 0x4d, 0x51, 0x58];
        apply_mask(&mut data, key, 0);
        assert_eq!(&data, b"Hello", "пример из RFC 6455, раздел 5.7");
        apply_mask(&mut data, key, 0);
        assert_eq!(data, [0x7f, 0x9f, 0x4d, 0x51, 0x58], "повторное применение возвращает исходное");
    }

    #[test]
    fn empty_data_is_fine() {
        let mut data: [u8; 0] = [];
        apply_mask(&mut data, [1, 2, 3, 4], 0);
        apply_mask(&mut data, [1, 2, 3, 4], 7);
    }

    #[test]
    fn key_repeats_around() {
        let mut data = [0u8; 10];
        apply_mask(&mut data, [1, 2, 3, 4], 0);
        assert_eq!(data, [1, 2, 3, 4, 1, 2, 3, 4, 1, 2]);
    }

    #[test]
    fn offset_continues_where_the_previous_piece_stopped() {
        let key = [0xde, 0xad, 0xbe, 0xef];
        let plain: Vec<u8> = (0..37u8).collect();
        let mut whole = plain.clone();
        apply_mask(&mut whole, key, 0);
        for cut in 0..=plain.len() {
            let mut parts = plain.clone();
            let (a, b) = parts.split_at_mut(cut);
            apply_mask(a, key, 0);
            apply_mask(b, key, cut);
            assert_eq!(parts, whole, "кусок данных, разрезанный в позиции {cut}: второй кусок надо маскировать со смещением {cut}, а не с нуля");
        }
    }

    #[test]
    fn huge_offsets_do_not_overflow() {
        let mut data = [0u8; 4];
        apply_mask(&mut data, [1, 2, 3, 4], usize::MAX - 1);
        let first = (usize::MAX - 1) % 4;
        assert_eq!(data[0], [1, 2, 3, 4][first]);
    }

    #[test]
    fn who_must_mask() {
        assert_eq!(check_mask(Role::Server, true), Ok(()), "сервер получает от клиента маскированные фреймы");
        assert_eq!(check_mask(Role::Server, false), Err(ProtoError::MaskRequired), "клиент прислал немаскированный фрейм: нарушение протокола");
        assert_eq!(check_mask(Role::Client, false), Ok(()), "клиент получает от сервера немаскированные");
        assert_eq!(check_mask(Role::Client, true), Err(ProtoError::MaskForbidden), "сервер замаскировал фрейм: нарушение протокола");
    }
}
