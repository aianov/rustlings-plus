// Размеры, порты и номера страниц приходят из сети байтами. Переводить их в `String` ради `parse` значит
// выделять память на каждое число и принимать лишнее: знак, пробелы. Десятичное число разбирается за один
// проход по байтам: цифра это `b - b'0'`, накопление `n * 10 + digit`. Переполнение здесь не авария, а
// ожидаемая ошибка входа, поэтому арифметика проверяемая (`checked_mul`, `checked_add`).
// TODO: Допиши `parse_decimal` и `parse_port` (порт это число от 0 до 65535).
#[derive(Debug, PartialEq)]
pub enum NumError {
    Empty,
    BadByte(u8),
    Overflow,
}

pub fn parse_decimal(bytes: &[u8]) -> Result<u64, NumError> {
    todo!("разбор десятичного числа без String")
}

pub fn parse_port(bytes: &[u8]) -> Result<u16, NumError> {
    todo!("число, которое помещается в u16")
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
