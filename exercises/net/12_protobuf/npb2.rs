// Теперь в обратную сторону: из потока байтов нужно достать число и понять, сколько байтов оно заняло. Данные приходят
// от чужого сервиса, поэтому ждать от них аккуратности нельзя: поток может оборваться на полуслове, а злой отправитель
// пришлёт сотню байтов с единичным старшим битом, чтобы число не кончалось никогда.
// TODO: Допиши decode_varint: число и количество прочитанных байтов, либо подробная ошибка. Паниковать нельзя ни на каком входе.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum VarintError {
    Truncated,
    TooLong,
    Overflow,
}

fn decode_varint(buf: &[u8]) -> Result<(u64, usize), VarintError> {
    let _ = buf;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let low = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(low);
                return out;
            }
            out.push(low | 0x80);
        }
    }

    #[test]
    fn known_vectors_and_consumed_length() {
        assert_eq!(decode_varint(&[0x00]), Ok((0, 1)));
        assert_eq!(decode_varint(&[0x96, 0x01]), Ok((150, 2)));
        assert_eq!(decode_varint(&[0xac, 0x02, 0xff, 0xff]), Ok((300, 2)), "хвост после числа не трогаем, а длину возвращаем");
        assert_eq!(decode_varint(&[0xff, 0xff, 0xff, 0xff, 0x0f]), Ok((u32::MAX as u64, 5)));
        let max = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(decode_varint(&max), Ok((u64::MAX, 10)));
    }

    #[test]
    fn roundtrip_on_many_values() {
        let mut x: u64 = 12345;
        for _ in 0..3000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let v = x >> (x % 64);
            let mut bytes = encode(v);
            let len = bytes.len();
            bytes.extend_from_slice(&[0xaa, 0xbb]);
            assert_eq!(decode_varint(&bytes), Ok((v, len)));
        }
    }

    #[test]
    fn truncated_input_is_an_error() {
        assert_eq!(decode_varint(&[]), Err(VarintError::Truncated), "пустой вход");
        assert_eq!(decode_varint(&[0x80]), Err(VarintError::Truncated));
        assert_eq!(decode_varint(&[0xff, 0xff, 0xff]), Err(VarintError::Truncated), "поток оборвался, а флаг продолжения стоял");
        for cut in 0..10 {
            let full = encode(u64::MAX);
            assert_eq!(decode_varint(&full[..cut]), Err(VarintError::Truncated), "обрезанный на {cut} байтах u64::MAX");
        }
    }

    #[test]
    fn endless_varint_is_too_long() {
        let endless = vec![0x80u8; 100];
        assert_eq!(decode_varint(&endless), Err(VarintError::TooLong), "сотня байтов с флагом продолжения");
        let ten_with_flag = vec![0xffu8; 10];
        assert_eq!(decode_varint(&ten_with_flag), Err(VarintError::TooLong), "на десятом байте число обязано закончиться");
        let eleven = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        assert_eq!(decode_varint(&eleven), Err(VarintError::TooLong));
    }

    #[test]
    fn value_that_does_not_fit_in_u64_is_overflow() {
        let mut bytes = vec![0xffu8; 9];
        bytes.push(0x02);
        assert_eq!(decode_varint(&bytes), Err(VarintError::Overflow), "десятый байт может нести только один значащий бит");
        let mut bytes = vec![0x80u8; 9];
        bytes.push(0x7f);
        assert_eq!(decode_varint(&bytes), Err(VarintError::Overflow));
    }

    #[test]
    fn non_minimal_encodings_are_accepted() {
        assert_eq!(decode_varint(&[0x80, 0x00]), Ok((0, 2)), "protobuf терпит лишние нулевые байты, мы тоже");
        assert_eq!(decode_varint(&[0x81, 0x80, 0x80, 0x00]), Ok((1, 4)));
        let mut padded = vec![0x80u8; 9];
        padded.push(0x00);
        assert_eq!(decode_varint(&padded), Ok((0, 10)));
    }
}
