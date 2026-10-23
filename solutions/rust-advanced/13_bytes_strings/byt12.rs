#[derive(Debug, PartialEq)]
pub enum LenError {
    Empty,
    NotDigits,
    TooLarge,
}

pub fn content_length(value: &[u8]) -> Result<usize, LenError> {
    if value.is_empty() {
        return Err(LenError::Empty);
    }
    let mut n: usize = 0;
    for &b in value {
        if !b.is_ascii_digit() {
            return Err(LenError::NotDigits);
        }
        n = n.checked_mul(10).and_then(|n| n.checked_add(usize::from(b - b'0'))).ok_or(LenError::TooLarge)?;
    }
    Ok(n)
}

pub fn chunk_size(hex: &[u8]) -> Result<u32, LenError> {
    if hex.is_empty() {
        return Err(LenError::Empty);
    }
    let mut n: u32 = 0;
    for &b in hex {
        let digit = char::from(b).to_digit(16).ok_or(LenError::NotDigits)?;
        n = n.checked_mul(16).and_then(|n| n.checked_add(digit)).ok_or(LenError::TooLarge)?;
    }
    Ok(n)
}

fn main() {
    println!("{:?}", content_length(b"128"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_length_accepts_only_digits() {
        assert_eq!(content_length(b"0"), Ok(0));
        assert_eq!(content_length(b"128"), Ok(128));
        assert_eq!(content_length(b""), Err(LenError::Empty));
        assert_eq!(content_length(b"+5"), Err(LenError::NotDigits), "знак плюс стандартный parse пропускает, а мы нет");
        assert_eq!(content_length(b" 5"), Err(LenError::NotDigits), "пробелы убирает вызывающий, не эта функция");
        assert_eq!(content_length(b"5 "), Err(LenError::NotDigits));
        assert_eq!(content_length(b"1_0"), Err(LenError::NotDigits));
        assert_eq!(content_length(b"-1"), Err(LenError::NotDigits));
        assert_eq!(content_length(b"0x10"), Err(LenError::NotDigits));
    }

    #[test]
    fn a_huge_content_length_is_too_large() {
        assert_eq!(content_length(b"99999999999999999999999999"), Err(LenError::TooLarge));
        assert_eq!(content_length(b"18446744073709551615"), Ok(usize::MAX));
        assert_eq!(content_length(b"18446744073709551616"), Err(LenError::TooLarge));
    }

    #[test]
    fn chunk_sizes_are_hex() {
        assert_eq!(chunk_size(b"0"), Ok(0));
        assert_eq!(chunk_size(b"1a"), Ok(26));
        assert_eq!(chunk_size(b"FF"), Ok(255));
        assert_eq!(chunk_size(b"ffffffff"), Ok(u32::MAX));
        assert_eq!(chunk_size(b""), Err(LenError::Empty));
        assert_eq!(chunk_size(b"g1"), Err(LenError::NotDigits));
        assert_eq!(chunk_size(b"+1"), Err(LenError::NotDigits));
    }

    #[test]
    fn a_long_hex_number_does_not_wrap() {
        assert_eq!(chunk_size(b"100000000"), Err(LenError::TooLarge), "это 2^32: в u32 не помещается, а не «ноль»");
        assert_eq!(chunk_size(b"1ffffffff"), Err(LenError::TooLarge));
        assert_eq!(chunk_size(b"0000000000000001"), Ok(1));
    }
}
