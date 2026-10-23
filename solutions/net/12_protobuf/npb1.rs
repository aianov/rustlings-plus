// В protobuf числа почти никогда не занимают положенные четыре или восемь байт: единица должна уместиться в один байт,
// триста в два, а самое большое u64 в десять. Для этого есть кодировка varint, и все остальные части формата стоят на ней.
// TODO: Допиши encode_varint, которая дописывает число в конец вектора, и varint_len, которая говорит, сколько байтов
// займёт число, не записывая его.

#![allow(dead_code)]

fn encode_varint(value: u64, out: &mut Vec<u8>) {
    let mut rest = value;
    loop {
        let low = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

fn varint_len(value: u64) -> usize {
    let bits = 64 - (value | 1).leading_zeros() as usize;
    bits.div_ceil(7)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        encode_varint(v, &mut out);
        out
    }

    #[test]
    fn small_numbers_take_one_byte() {
        for v in 0..=127u64 {
            assert_eq!(enc(v), vec![v as u8], "число {v} должно уместиться в один байт без флага продолжения");
        }
    }

    #[test]
    fn known_vectors() {
        let cases: [(u64, &[u8]); 8] = [
            (128, &[0x80, 0x01]),
            (150, &[0x96, 0x01]),
            (300, &[0xac, 0x02]),
            (16_383, &[0xff, 0x7f]),
            (16_384, &[0x80, 0x80, 0x01]),
            (u32::MAX as u64, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
            (1 << 35, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]),
            (u64::MAX, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]),
        ];
        for (value, expected) in cases {
            assert_eq!(enc(value), expected, "неверные байты для {value}");
        }
    }

    #[test]
    fn appends_instead_of_overwriting() {
        let mut out = vec![9, 9];
        encode_varint(1, &mut out);
        encode_varint(300, &mut out);
        assert_eq!(out, vec![9, 9, 1, 0xac, 0x02], "число дописывается в конец, прежнее содержимое остаётся");
    }

    #[test]
    fn every_byte_but_the_last_carries_the_continuation_flag() {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..2000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let v = x >> (x % 64);
            let bytes = enc(v);
            let (last, init) = bytes.split_last().unwrap();
            assert_eq!(last & 0x80, 0, "у последнего байта {v} флаг продолжения должен быть сброшен");
            assert!(init.iter().all(|b| b & 0x80 != 0), "у непоследних байтов {v} флаг продолжения должен стоять");
            if bytes.len() > 1 {
                assert_ne!(*last, 0, "лишний нулевой байт в конце: кодировка {v} не самая короткая");
            }
        }
    }

    #[test]
    fn varint_len_agrees_with_the_encoder() {
        for shift in 0..64 {
            for v in [(1u64 << shift) - 1, 1 << shift, (1 << shift) + 1] {
                assert_eq!(varint_len(v), enc(v).len(), "varint_len({v}) не совпадает с настоящей длиной");
            }
        }
        assert_eq!(varint_len(0), 1, "нуль тоже занимает один байт");
        assert_eq!(varint_len(u64::MAX), 10);
    }
}
