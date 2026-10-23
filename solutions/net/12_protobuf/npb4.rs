// Отрицательные числа в varint ведут себя неожиданно. Тип int32 с числом -1 в protobuf занимает десять байтов, хотя 1 занимает
// один, а sint32 с тем же -1 снова один байт. Если выбрать не тот тип для поля «изменение рейтинга», каждое отрицательное
// значение будет стоить в десять раз дороже. TODO: Допиши запись и чтение обоих типов, а также zigzag, на котором стоит sint.
// Для int32 отрицательное число расширяется знаком до 64 бит, как того требует формат.

#![allow(dead_code)]

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

fn get_varint(buf: &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for (i, &byte) in buf.iter().take(10).enumerate() {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn zigzag32(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

fn unzigzag32(z: u32) -> i32 {
    ((z >> 1) as i32) ^ -((z & 1) as i32)
}

fn int32_bytes(n: i32) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(i64::from(n) as u64, &mut out);
    out
}

fn sint32_bytes(n: i32) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(u64::from(zigzag32(n)), &mut out);
    out
}

fn read_int32(buf: &[u8]) -> Option<i32> {
    get_varint(buf).map(|v| v as i32)
}

fn read_sint32(buf: &[u8]) -> Option<i32> {
    get_varint(buf).map(|v| unzigzag32(v as u32))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const EDGES: [i32; 9] = [0, 1, -1, 2, -2, 63, -64, i32::MAX, i32::MIN];

    #[test]
    fn minus_one_in_int32_takes_ten_bytes() {
        assert_eq!(int32_bytes(-1), vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01], "отрицательный int32 расширяется знаком до 64 бит");
        assert_eq!(int32_bytes(1), vec![0x01]);
        assert_eq!(int32_bytes(i32::MIN).len(), 10);
        assert_eq!(int32_bytes(-2).len(), 10);
    }

    #[test]
    fn sint32_keeps_small_negatives_small() {
        assert_eq!(sint32_bytes(0), vec![0]);
        assert_eq!(sint32_bytes(-1), vec![1]);
        assert_eq!(sint32_bytes(1), vec![2]);
        assert_eq!(sint32_bytes(-2), vec![3]);
        assert_eq!(sint32_bytes(2), vec![4]);
        assert_eq!(sint32_bytes(-64), vec![127]);
        assert_eq!(sint32_bytes(64), vec![0x80, 0x01]);
        assert_eq!(sint32_bytes(i32::MAX), vec![0xfe, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(sint32_bytes(i32::MIN), vec![0xff, 0xff, 0xff, 0xff, 0x0f], "i32::MIN: abs() здесь переполнится, zigzag обязан обойтись без него");
    }

    #[test]
    fn zigzag_table_and_inverse() {
        let table = [(0, 0u32), (-1, 1), (1, 2), (-2, 3), (2147483647, 4294967294), (-2147483648, 4294967295)];
        for (n, z) in table {
            assert_eq!(zigzag32(n), z, "zigzag({n})");
            assert_eq!(unzigzag32(z), n, "unzigzag({z})");
        }
    }

    #[test]
    fn roundtrips_for_both_types() {
        let mut x: u64 = 99;
        let mut values = EDGES.to_vec();
        for _ in 0..2000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            values.push((x >> 32) as i32);
        }
        for n in values {
            assert_eq!(read_int32(&int32_bytes(n)), Some(n), "int32 {n}");
            assert_eq!(read_sint32(&sint32_bytes(n)), Some(n), "sint32 {n}");
            assert_eq!(unzigzag32(zigzag32(n)), n);
        }
    }

    #[test]
    fn int32_reader_takes_the_low_32_bits() {
        let five_byte_minus_one = [0xff, 0xff, 0xff, 0xff, 0x0f];
        assert_eq!(read_int32(&five_byte_minus_one), Some(-1), "некоторые кодировщики пишут отрицательный int32 пятью байтами, читаем младшие 32 бита");
        assert_eq!(read_int32(&[0x80]), None, "оборванное число");
    }

    #[test]
    fn mixing_up_the_types_gives_wrong_numbers() {
        assert_eq!(read_sint32(&int32_bytes(-1)), Some(i32::MIN), "-1, записанный как int32, но прочитанный как sint32, превращается в другое число");
        assert_eq!(read_int32(&sint32_bytes(-1)), Some(1));
    }
}
