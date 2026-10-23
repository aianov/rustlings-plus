// Строки, байты и вложенные сообщения записываются одинаково: длина, затем столько байтов. Длина тоже varint. Просто и
// удобно, пока длина честная. Но её присылает собеседник, и однажды он напишет, что строка занимает восемнадцать
// квинтиллионов байтов. TODO: Допиши put_len, take_len и take_string. Проверка длины должна случаться до того, как мы
// выйдем за границы буфера или попросим памяти. Строки обязаны быть корректным UTF-8.

#![allow(dead_code)]

#[derive(Debug, PartialEq)]
enum LenError {
    BadVarint,
    Truncated,
    InvalidUtf8,
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

fn put_len(payload: &[u8], out: &mut Vec<u8>) {
    let _ = (payload, out);
    todo!()
}

fn take_len(buf: &[u8]) -> Result<(&[u8], usize), LenError> {
    let _ = buf;
    todo!()
}

fn take_string(buf: &[u8]) -> Result<(&str, usize), LenError> {
    let _ = buf;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_roundtrip() {
        let mut out = Vec::new();
        put_len("привет".as_bytes(), &mut out);
        assert_eq!(out[0], 12, "длина считается в байтах, не в символах");
        assert_eq!(take_string(&out), Ok(("привет", 13)));
        let mut empty = Vec::new();
        put_len(b"", &mut empty);
        assert_eq!(empty, vec![0]);
        assert_eq!(take_string(&empty), Ok(("", 1)), "пустая строка занимает один байт");
    }

    #[test]
    fn nested_message_is_just_bytes_with_a_length() {
        let inner = [0x08, 0x96, 0x01];
        let mut outer = Vec::new();
        put_len(&inner, &mut outer);
        assert_eq!(outer, vec![0x03, 0x08, 0x96, 0x01]);
        let (payload, used) = take_len(&outer).unwrap();
        assert_eq!((payload, used), (&inner[..], 4));
        let mut long = Vec::new();
        put_len(&vec![7u8; 300], &mut long);
        assert_eq!(&long[..2], &[0xac, 0x02], "длина 300 это двухбайтный varint");
        assert_eq!(take_len(&long).map(|(p, u)| (p.len(), u)), Ok((300, 302)));
    }

    #[test]
    fn trailing_bytes_stay_untouched() {
        assert_eq!(take_len(&[2, b'o', b'k', b'!', b'!']), Ok((&b"ok"[..], 3)));
    }

    #[test]
    fn declared_length_beyond_the_buffer_is_truncated() {
        assert_eq!(take_len(&[5, b'a', b'b']), Err(LenError::Truncated));
        assert_eq!(take_len(&[1]), Err(LenError::Truncated));
        assert_eq!(take_len(&[]), Err(LenError::BadVarint));
        assert_eq!(take_len(&[0x80]), Err(LenError::BadVarint), "длина оборвалась");
    }

    #[test]
    fn absurd_lengths_do_not_panic_or_allocate() {
        let max = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, b'x'];
        assert_eq!(take_len(&max), Err(LenError::Truncated), "u64::MAX байтов в буфере из одиннадцати");
        let nearly = [0xf9, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, b'x'];
        assert_eq!(take_len(&nearly), Err(LenError::Truncated), "значение рядом с границей usize не должно давать переполнения при сложении");
    }

    #[test]
    fn invalid_utf8_is_rejected_for_strings_but_not_for_bytes() {
        let bad = [2, 0xff, 0xfe];
        assert_eq!(take_string(&bad), Err(LenError::InvalidUtf8));
        assert_eq!(take_len(&bad), Ok((&[0xff, 0xfe][..], 3)), "bytes принимает любые байты");
        let cut_in_char = [1, 0xd0];
        assert_eq!(take_string(&cut_in_char), Err(LenError::InvalidUtf8), "половина символа");
    }
}
