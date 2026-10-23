// Заголовки в HTTP/2 сжимаются форматом HPACK, и весь он построен на одной хитрой записи целых чисел. Число может делить
// байт с флагами: в первых битах байта живут признаки вида записи, а в оставшихся N битах («префиксе») начинается само число.
// Если число не влезает в префикс, префикс заполняется единицами, а остаток идёт следующими байтами по семь бит, как в varint.
// TODO: Допиши encode_int и decode_int для префиксов от 1 до 8 бит. Чужой вход не должен вызывать панику или переполнение.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum IntError {
    Truncated,
    TooLarge,
}

fn encode_int(value: u64, prefix_bits: u8, flags: u8, out: &mut Vec<u8>) {
    let max = (1u64 << prefix_bits) - 1;
    if value < max {
        out.push(flags | value as u8);
        return;
    }
    out.push(flags | max as u8);
    let mut rest = value - max;
    while rest >= 128 {
        out.push((rest % 128) as u8 | 0x80);
        rest /= 128;
    }
    out.push(rest as u8);
}

fn decode_int(buf: &[u8], prefix_bits: u8) -> Result<(u64, usize), IntError> {
    let first = *buf.first().ok_or(IntError::Truncated)?;
    let max = (1u64 << prefix_bits) - 1;
    let mut value = u64::from(first) & max;
    if value < max {
        return Ok((value, 1));
    }
    let mut shift = 0u32;
    for (i, &byte) in buf[1..].iter().enumerate() {
        let part = u64::from(byte & 0x7f);
        if shift > 56 || (part << shift) >> shift != part {
            return Err(IntError::TooLarge);
        }
        value = value.checked_add(part << shift).ok_or(IntError::TooLarge)?;
        if byte & 0x80 == 0 {
            return Ok((value, i + 2));
        }
        shift += 7;
    }
    Err(IntError::Truncated)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(value: u64, bits: u8, flags: u8) -> Vec<u8> {
        let mut out = Vec::new();
        encode_int(value, bits, flags, &mut out);
        out
    }

    #[test]
    fn rfc_examples() {
        assert_eq!(enc(10, 5, 0), vec![0b0000_1010], "RFC 7541, C.1.1: 10 в 5-битном префиксе");
        assert_eq!(enc(1337, 5, 0), vec![0b0001_1111, 0b1001_1010, 0b0000_1010], "RFC 7541, C.1.2: 1337 в 5-битном префиксе");
        assert_eq!(enc(42, 8, 0), vec![42], "RFC 7541, C.1.3: 42 в 8-битном префиксе");
    }

    #[test]
    fn flags_stay_in_the_top_bits() {
        assert_eq!(enc(2, 7, 0x80), vec![0x82], "индексированное поле: флаг 1 и индекс 2");
        assert_eq!(enc(4, 6, 0x40), vec![0x44]);
        assert_eq!(enc(200, 7, 0x80), vec![0xff, 0x49], "значение не влезло в префикс: 127 в первом байте, остаток 73 следом");
    }

    #[test]
    fn boundary_between_one_byte_and_two() {
        assert_eq!(enc(30, 5, 0), vec![30]);
        assert_eq!(enc(31, 5, 0), vec![31, 0], "значение, равное максимуму префикса, уже требует продолжения: иначе читатель не отличит его от «больше»");
        assert_eq!(enc(32, 5, 0), vec![31, 1]);
        assert_eq!(decode_int(&[31, 0], 5), Ok((31, 2)));
    }

    #[test]
    fn decode_examples_and_consumed_length() {
        assert_eq!(decode_int(&[0b0000_1010, 99], 5), Ok((10, 1)));
        assert_eq!(decode_int(&[0b0001_1111, 0b1001_1010, 0b0000_1010, 99], 5), Ok((1337, 3)));
        assert_eq!(decode_int(&[0xe5, 99], 5), Ok((5, 1)), "биты флагов в первом байте игнорируются: читаем только префикс");
        assert_eq!(decode_int(&[0xff, 0x49], 7), Ok((200, 2)));
    }

    #[test]
    fn roundtrip_for_all_prefixes() {
        let values = [0u64, 1, 30, 31, 126, 127, 128, 254, 255, 256, 1337, 16_383, 16_384, 1 << 31, (1 << 32) + 5, u64::MAX / 2];
        for bits in 1..=8u8 {
            for &value in &values {
                let flags = if bits < 8 { 0xffu8 << bits } else { 0 };
                let mut bytes = enc(value, bits, flags);
                let len = bytes.len();
                bytes.push(0xaa);
                assert_eq!(decode_int(&bytes, bits), Ok((value, len)), "префикс {bits} бит, значение {value}");
            }
        }
    }

    #[test]
    fn truncated_input() {
        assert_eq!(decode_int(&[], 5), Err(IntError::Truncated));
        assert_eq!(decode_int(&[0x1f], 5), Err(IntError::Truncated), "префикс полон, а продолжения нет");
        assert_eq!(decode_int(&[0x1f, 0x80], 5), Err(IntError::Truncated), "продолжение обещано и не пришло");
    }

    #[test]
    fn absurdly_long_continuations_are_rejected_without_panic() {
        let mut endless = vec![0x1f];
        endless.extend(std::iter::repeat_n(0xff, 40));
        assert_eq!(decode_int(&endless, 5), Err(IntError::TooLarge), "сдвиг больше 64 бит не должен паниковать");
        let mut big = vec![0xffu8];
        big.extend_from_slice(&[0xff; 9]);
        big.push(0x7f);
        assert_eq!(decode_int(&big, 8), Err(IntError::TooLarge), "число не влезает в u64");
    }
}
