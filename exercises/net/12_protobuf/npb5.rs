// Не всё в protobuf varint. Для чисел, которые обычно велики (хэши, идентификаторы на весь диапазон, координаты), есть
// поля фиксированной ширины: четыре или восемь байтов. К сетевому порядку байтов они не имеют отношения: протокол выбрал
// другой. TODO: Допиши запись и чтение fixed32, fixed64, float и double, а также знаковые sfixed32 и sfixed64.
// Нехватку байтов в чтении сообщай ошибкой, лишние байты в конце оставляй в покое.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
struct Short;

fn put_fixed32(v: u32, out: &mut Vec<u8>) {
    let _ = (v, out);
    todo!()
}

fn put_fixed64(v: u64, out: &mut Vec<u8>) {
    let _ = (v, out);
    todo!()
}

fn put_float(v: f32, out: &mut Vec<u8>) {
    let _ = (v, out);
    todo!()
}

fn put_double(v: f64, out: &mut Vec<u8>) {
    let _ = (v, out);
    todo!()
}

fn get_fixed32(buf: &[u8]) -> Result<u32, Short> {
    let _ = buf;
    todo!()
}

fn get_fixed64(buf: &[u8]) -> Result<u64, Short> {
    let _ = buf;
    todo!()
}

fn get_sfixed32(buf: &[u8]) -> Result<i32, Short> {
    let _ = buf;
    todo!()
}

fn get_sfixed64(buf: &[u8]) -> Result<i64, Short> {
    let _ = buf;
    todo!()
}

fn get_float(buf: &[u8]) -> Result<f32, Short> {
    let _ = buf;
    todo!()
}

fn get_double(buf: &[u8]) -> Result<f64, Short> {
    let _ = buf;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn out32(v: u32) -> Vec<u8> {
        let mut out = Vec::new();
        put_fixed32(v, &mut out);
        out
    }

    #[test]
    fn integers_are_little_endian() {
        assert_eq!(out32(1), vec![1, 0, 0, 0], "младший байт первым, это не сетевой порядок");
        assert_eq!(out32(0x0102_0304), vec![4, 3, 2, 1]);
        let mut out = Vec::new();
        put_fixed64(0x0102_0304_0506_0708, &mut out);
        assert_eq!(out, vec![8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn floats_follow_ieee_754_in_little_endian() {
        let mut out = Vec::new();
        put_float(1.0, &mut out);
        assert_eq!(out, vec![0x00, 0x00, 0x80, 0x3f]);
        let mut out = Vec::new();
        put_double(1.0, &mut out);
        assert_eq!(out, vec![0, 0, 0, 0, 0, 0, 0xf0, 0x3f]);
        let mut out = Vec::new();
        put_double(-2.5, &mut out);
        assert_eq!(get_double(&out), Ok(-2.5));
    }

    #[test]
    fn signed_variants_reinterpret_the_same_bytes() {
        assert_eq!(get_sfixed32(&[0xff, 0xff, 0xff, 0xff]), Ok(-1));
        assert_eq!(get_sfixed32(&out32(0x8000_0000)), Ok(i32::MIN));
        assert_eq!(get_sfixed64(&[0xff; 8]), Ok(-1));
        assert_eq!(get_fixed32(&[0xff; 4]), Ok(u32::MAX));
        assert_eq!(get_fixed64(&[0xff; 8]), Ok(u64::MAX));
    }

    #[test]
    fn special_float_values_keep_their_bits() {
        for bits in [0x8000_0000u32, 0x7fc0_0001, 0xffc0_1234, 0x7f80_0000, 0xff80_0000, 1] {
            let mut out = Vec::new();
            put_float(f32::from_bits(bits), &mut out);
            assert_eq!(get_float(&out).unwrap().to_bits(), bits, "биты {bits:#x} должны дойти без изменений (минус ноль, NaN с нагрузкой, бесконечности)");
        }
        for bits in [0x8000_0000_0000_0000u64, 0x7ff8_0000_0000_0001, 0xfff0_0000_0000_0000] {
            let mut out = Vec::new();
            put_double(f64::from_bits(bits), &mut out);
            assert_eq!(get_double(&out).unwrap().to_bits(), bits);
        }
    }

    #[test]
    fn short_input_is_an_error_and_extra_bytes_are_ignored() {
        assert_eq!(get_fixed32(&[1, 2, 3]), Err(Short));
        assert_eq!(get_fixed64(&[1, 2, 3, 4, 5, 6, 7]), Err(Short));
        assert_eq!(get_float(&[]), Err(Short));
        assert_eq!(get_double(&[0; 7]), Err(Short));
        assert_eq!(get_fixed32(&[1, 0, 0, 0, 99, 99]), Ok(1), "читаем ровно четыре байта, остальное не наше");
        assert_eq!(get_sfixed64(&[1, 0, 0, 0, 0, 0, 0, 0, 5]), Ok(1));
    }

    #[test]
    fn roundtrip_random() {
        let mut x: u64 = 7;
        for _ in 0..1000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let mut out = Vec::new();
            put_fixed64(x, &mut out);
            put_fixed32((x >> 7) as u32, &mut out);
            assert_eq!(get_fixed64(&out), Ok(x));
            assert_eq!(get_fixed32(&out[8..]), Ok((x >> 7) as u32));
        }
    }
}
