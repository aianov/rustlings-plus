#[derive(Debug, PartialEq)]
pub enum NumError {
    Empty,
    BadByte(u8),
    Overflow,
}

pub fn parse_decimal(bytes: &[u8]) -> Result<u64, NumError> {
    if bytes.is_empty() {
        return Err(NumError::Empty);
    }
    let mut n: u64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return Err(NumError::BadByte(b));
        }
        n = n.checked_mul(10).and_then(|n| n.checked_add(u64::from(b - b'0'))).ok_or(NumError::Overflow)?;
    }
    Ok(n)
}

pub fn parse_port(bytes: &[u8]) -> Result<u16, NumError> {
    u16::try_from(parse_decimal(bytes)?).map_err(|_| NumError::Overflow)
}

fn main() {
    println!("{:?}", parse_decimal(b"8080"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_numbers() {
        assert_eq!(parse_decimal(b"0"), Ok(0));
        assert_eq!(parse_decimal(b"42"), Ok(42));
        assert_eq!(parse_decimal(b"007"), Ok(7));
        assert_eq!(parse_decimal(b"18446744073709551615"), Ok(u64::MAX));
    }

    #[test]
    fn empty_and_bad_bytes_are_told_apart() {
        assert_eq!(parse_decimal(b""), Err(NumError::Empty));
        assert_eq!(parse_decimal(b"12a4"), Err(NumError::BadByte(b'a')));
        assert_eq!(parse_decimal(b" 5"), Err(NumError::BadByte(b' ')));
        assert_eq!(parse_decimal(&[b'1', 0xff]), Err(NumError::BadByte(0xff)), "байт вне ASCII это просто плохой байт");
    }

    #[test]
    fn overflow_is_an_error_not_a_wrap() {
        assert_eq!(parse_decimal(b"18446744073709551616"), Err(NumError::Overflow));
        assert_eq!(parse_decimal(b"99999999999999999999999"), Err(NumError::Overflow));
        assert_eq!(parse_decimal(b"00000000000000000000000000001"), Ok(1), "ведущие нули переполнением не считаются");
    }

    #[test]
    fn ports_have_their_own_range() {
        assert_eq!(parse_port(b"8080"), Ok(8080));
        assert_eq!(parse_port(b"65535"), Ok(65535));
        assert_eq!(parse_port(b"65536"), Err(NumError::Overflow));
        assert_eq!(parse_port(b"x"), Err(NumError::BadByte(b'x')));
        assert_eq!(parse_port(b""), Err(NumError::Empty));
    }
}
