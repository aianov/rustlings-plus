// Клиент gRPC сообщает серверу, сколько времени у вызова осталось, заголовком grpc-timeout. Значение это целое до восьми цифр и
// буква единицы: H (часы), M (минуты), S (секунды), m (миллисекунды), u (микросекунды), n (наносекунды). Сервер превращает
// его в крайний срок и обязан бросить работу, когда срок вышел. Заголовок приходит от клиента, поэтому формат нужно проверять строго.
// TODO: Допиши parse_timeout и format_timeout. Для записи бери самую мелкую единицу, в которой число укладывается в восемь цифр,
// с округлением вверх: срок нельзя укорачивать.

#![allow(dead_code)]
use std::time::Duration;

#[derive(Debug, PartialEq)]
enum TimeoutError {
    Empty,
    NoDigits,
    TooManyDigits,
    BadUnit(char),
    NotADigit,
}

fn parse_timeout(text: &str) -> Result<Duration, TimeoutError> {
    let unit = text.chars().last().ok_or(TimeoutError::Empty)?;
    let digits = &text[..text.len() - unit.len_utf8()];
    if !matches!(unit, 'H' | 'M' | 'S' | 'm' | 'u' | 'n') {
        return Err(TimeoutError::BadUnit(unit));
    }
    if digits.is_empty() {
        return Err(TimeoutError::NoDigits);
    }
    if digits.len() > 8 {
        return Err(TimeoutError::TooManyDigits);
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TimeoutError::NotADigit);
    }
    let n: u64 = digits.parse().map_err(|_| TimeoutError::NotADigit)?;
    Ok(match unit {
        'H' => Duration::from_secs(n * 3600),
        'M' => Duration::from_secs(n * 60),
        'S' => Duration::from_secs(n),
        'm' => Duration::from_millis(n),
        'u' => Duration::from_micros(n),
        _ => Duration::from_nanos(n),
    })
}

fn format_timeout(d: Duration) -> String {
    const LIMIT: u128 = 99_999_999;
    let nanos = d.as_nanos();
    let units: [(u128, char); 6] = [(1, 'n'), (1_000, 'u'), (1_000_000, 'm'), (1_000_000_000, 'S'), (60_000_000_000, 'M'), (3_600_000_000_000, 'H')];
    for (size, letter) in units {
        let value = nanos.div_ceil(size);
        if value <= LIMIT {
            return format!("{value}{letter}");
        }
    }
    format!("{LIMIT}H")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unit() {
        assert_eq!(parse_timeout("2H"), Ok(Duration::from_secs(7200)));
        assert_eq!(parse_timeout("5M"), Ok(Duration::from_secs(300)));
        assert_eq!(parse_timeout("5S"), Ok(Duration::from_secs(5)));
        assert_eq!(parse_timeout("100m"), Ok(Duration::from_millis(100)));
        assert_eq!(parse_timeout("250u"), Ok(Duration::from_micros(250)));
        assert_eq!(parse_timeout("7n"), Ok(Duration::from_nanos(7)));
        assert_eq!(parse_timeout("0n"), Ok(Duration::ZERO), "нулевой срок допустим: вызов уже просрочен");
        assert_eq!(parse_timeout("00000042S"), Ok(Duration::from_secs(42)), "ведущие нули допустимы, если цифр не больше восьми");
    }

    #[test]
    fn units_are_case_sensitive() {
        assert_eq!(parse_timeout("1M"), Ok(Duration::from_secs(60)), "M это минуты");
        assert_eq!(parse_timeout("1m"), Ok(Duration::from_millis(1)), "m это миллисекунды");
        assert_eq!(parse_timeout("1h"), Err(TimeoutError::BadUnit('h')));
        assert_eq!(parse_timeout("1s"), Err(TimeoutError::BadUnit('s')));
        assert_eq!(parse_timeout("1N"), Err(TimeoutError::BadUnit('N')));
    }

    #[test]
    fn malformed_values() {
        assert_eq!(parse_timeout(""), Err(TimeoutError::Empty));
        assert_eq!(parse_timeout("S"), Err(TimeoutError::NoDigits));
        assert_eq!(parse_timeout("100"), Err(TimeoutError::BadUnit('0')), "без единицы последняя цифра принимается за единицу и отвергается");
        assert_eq!(parse_timeout("123456789S"), Err(TimeoutError::TooManyDigits), "девять цифр");
        assert_eq!(parse_timeout("-5S"), Err(TimeoutError::NotADigit));
        assert_eq!(parse_timeout("+5S"), Err(TimeoutError::NotADigit));
        assert_eq!(parse_timeout("1 S"), Err(TimeoutError::NotADigit));
        assert_eq!(parse_timeout(" 5S"), Err(TimeoutError::NotADigit));
        assert_eq!(parse_timeout("1.5S"), Err(TimeoutError::NotADigit));
        assert_eq!(parse_timeout("5Sm"), Err(TimeoutError::NotADigit), "единица одна и стоит в конце");
        assert_eq!(parse_timeout("٣S"), Err(TimeoutError::NotADigit), "цифры других алфавитов тоже не цифры");
    }

    #[test]
    fn eight_digits_is_the_limit() {
        assert_eq!(parse_timeout("99999999H"), Ok(Duration::from_secs(99_999_999 * 3600)));
        assert_eq!(parse_timeout("99999999S"), Ok(Duration::from_secs(99_999_999)));
        assert_eq!(parse_timeout("999999999S"), Err(TimeoutError::TooManyDigits));
    }

    #[test]
    fn formatting_picks_the_smallest_unit_that_fits_eight_digits() {
        assert_eq!(format_timeout(Duration::ZERO), "0n");
        assert_eq!(format_timeout(Duration::from_nanos(7)), "7n");
        assert_eq!(format_timeout(Duration::from_nanos(99_999_999)), "99999999n");
        assert_eq!(format_timeout(Duration::from_nanos(100_000_000)), "100000u", "100 мс не влезают в 8 цифр наносекунд, берём микросекунды");
        assert_eq!(format_timeout(Duration::from_secs(5)), "5000000u");
        assert_eq!(format_timeout(Duration::from_secs(100)), "100000m");
        assert_eq!(format_timeout(Duration::from_secs(100_000)), "100000S");
        assert_eq!(format_timeout(Duration::from_secs(100_000_000)), "1666667M", "округление вверх: 1666666,67 минуты");
        assert_eq!(format_timeout(Duration::from_secs(10_000_000_000)), "2777778H");
    }

    #[test]
    fn rounding_never_shortens_the_deadline() {
        assert_eq!(format_timeout(Duration::from_nanos(100_000_001)), "100001u", "100000,001 мкс округляется вверх");
        for n in [1u64, 12_345, 99_999_999, 123_456_789, 987_654_321_012, 5_555_555_555_555] {
            let d = Duration::from_nanos(n);
            let text = format_timeout(d);
            let back = parse_timeout(&text).unwrap();
            assert!(back >= d, "{text} короче исходного {d:?}");
            assert!(back - d < Duration::from_nanos(n / 1_000_000 + 1000), "{text} слишком сильно длиннее {d:?}");
            assert!(text.len() <= 9, "{text}: не больше восьми цифр и единица");
        }
    }

    #[test]
    fn huge_durations_are_clamped() {
        assert_eq!(format_timeout(Duration::from_secs(u64::MAX)), "99999999H");
        assert_eq!(parse_timeout(&format_timeout(Duration::from_secs(1 << 50))), Ok(Duration::from_secs(99_999_999 * 3600)));
    }
}
