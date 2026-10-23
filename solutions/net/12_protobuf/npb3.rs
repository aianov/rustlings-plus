// Сообщение protobuf это последовательность полей, а у каждого поля впереди ключ: один varint, в котором спрятаны номер поля
// и способ записи значения. Без схемы по ключу уже видно, сколько байтов занимает следующее значение и как его пропустить.
// Ключ приходит от чужого сервиса, поэтому с ним нужно обращаться строго: выдуманные способы записи и номер ноль это ошибки.
// TODO: Допиши parse_key и encode_key. Группы (устаревшие StartGroup и EndGroup) не поддерживаем и называем отдельной ошибкой.

#![allow(dead_code)]

#[derive(Debug, PartialEq, Clone, Copy)]
enum WireType {
    Varint,
    I64,
    Len,
    I32,
}

#[derive(Debug, PartialEq)]
struct Key {
    number: u32,
    wire: WireType,
}

#[derive(Debug, PartialEq)]
enum KeyError {
    BadVarint,
    ZeroField,
    FieldTooBig,
    BadWireType(u8),
    GroupsUnsupported,
}

const MAX_FIELD: u32 = (1 << 29) - 1;

fn get_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for (i, &byte) in buf.iter().take(10).enumerate() {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

fn put_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let low = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

fn parse_key(buf: &[u8]) -> Result<(Key, usize), KeyError> {
    let (raw, used) = get_varint(buf).ok_or(KeyError::BadVarint)?;
    let number = raw >> 3;
    if number == 0 {
        return Err(KeyError::ZeroField);
    }
    if number > u64::from(MAX_FIELD) {
        return Err(KeyError::FieldTooBig);
    }
    let wire = match (raw & 7) as u8 {
        0 => WireType::Varint,
        1 => WireType::I64,
        2 => WireType::Len,
        5 => WireType::I32,
        3 | 4 => return Err(KeyError::GroupsUnsupported),
        other => return Err(KeyError::BadWireType(other)),
    };
    Ok((Key { number: number as u32, wire }, used))
}

fn encode_key(number: u32, wire: WireType, out: &mut Vec<u8>) {
    let bits = match wire {
        WireType::Varint => 0,
        WireType::I64 => 1,
        WireType::Len => 2,
        WireType::I32 => 5,
    };
    put_varint((u64::from(number) << 3) | bits, out);
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(number: u32, wire: WireType) -> Result<(Key, usize), KeyError> {
        Ok((Key { number, wire }, 1))
    }

    #[test]
    fn small_field_numbers_fit_in_one_byte() {
        assert_eq!(parse_key(&[0x08]), key(1, WireType::Varint));
        assert_eq!(parse_key(&[0x12]), key(2, WireType::Len));
        assert_eq!(parse_key(&[0x09]), key(1, WireType::I64));
        assert_eq!(parse_key(&[0x0d]), key(1, WireType::I32));
        assert_eq!(parse_key(&[0x78]), key(15, WireType::Varint), "номера до 15 живут в одном байте ключа");
    }

    #[test]
    fn field_16_needs_two_bytes() {
        assert_eq!(parse_key(&[0x80, 0x01]), Ok((Key { number: 16, wire: WireType::Varint }, 2)));
        let mut out = Vec::new();
        encode_key(16, WireType::Varint, &mut out);
        assert_eq!(out, vec![0x80, 0x01]);
        out.clear();
        encode_key(15, WireType::Len, &mut out);
        assert_eq!(out, vec![0x7a]);
    }

    #[test]
    fn rest_of_the_buffer_is_left_alone() {
        assert_eq!(parse_key(&[0x12, 0x03, b'a', b'b', b'c']), key(2, WireType::Len));
    }

    #[test]
    fn invalid_keys_are_errors() {
        assert_eq!(parse_key(&[0x00]), Err(KeyError::ZeroField), "поле номер ноль не бывает");
        assert_eq!(parse_key(&[0x0e]), Err(KeyError::BadWireType(6)));
        assert_eq!(parse_key(&[0x0f]), Err(KeyError::BadWireType(7)));
        assert_eq!(parse_key(&[0x0b]), Err(KeyError::GroupsUnsupported));
        assert_eq!(parse_key(&[0x0c]), Err(KeyError::GroupsUnsupported));
        assert_eq!(parse_key(&[]), Err(KeyError::BadVarint));
        assert_eq!(parse_key(&[0x88]), Err(KeyError::BadVarint), "ключ оборвался");
    }

    #[test]
    fn the_biggest_legal_field_and_one_more() {
        assert_eq!(parse_key(&[0xf8, 0xff, 0xff, 0xff, 0x0f]), Ok((Key { number: MAX_FIELD, wire: WireType::Varint }, 5)));
        assert_eq!(parse_key(&[0x80, 0x80, 0x80, 0x80, 0x10]), Err(KeyError::FieldTooBig), "536870912 уже не помещается в 29 бит");
        let huge = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(parse_key(&huge), Err(KeyError::FieldTooBig), "огромный ключ не должен ни паниковать, ни обрезаться до u32");
    }

    #[test]
    fn encode_then_parse_roundtrip() {
        for number in [1, 2, 15, 16, 127, 128, 2047, 2048, 262_143, 262_144, MAX_FIELD] {
            for wire in [WireType::Varint, WireType::I64, WireType::Len, WireType::I32] {
                let mut out = Vec::new();
                encode_key(number, wire, &mut out);
                let (parsed, used) = parse_key(&out).unwrap();
                assert_eq!((parsed, used), (Key { number, wire }, out.len()), "поле {number} со способом {wire:?}");
            }
        }
    }
}
