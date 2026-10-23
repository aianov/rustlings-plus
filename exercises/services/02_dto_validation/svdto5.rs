// Ограничение «ник от 3 до 20 символов» должно считать то, что видит человек. Кириллический ник
// из десяти букв занимает двадцать байт, а смайлик это четыре байта. Пробелы по краям никому
// не нужны: «  ab  » это два символа, а не шесть. Напиши подсчёт длины и проверку диапазона.
// Договорённость курса: считаем скалярные значения Unicode (`char`), без учёта составных символов.

#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub code: &'static str,
    pub message: String,
}

pub fn char_len(raw: &str) -> usize {
    todo!()
}

pub fn check_len(field: &str, raw: &str, min: usize, max: usize) -> Option<FieldError> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_chars_not_bytes() {
        assert_eq!(char_len("привет"), 6, "шесть букв, хотя двенадцать байт");
        assert_eq!(char_len("👍👍"), 2, "смайлик это один символ");
        assert_eq!(char_len("日本語"), 3, "иероглифы тоже по одному");
    }

    #[test]
    fn whitespace_around_is_not_counted() {
        assert_eq!(char_len("  ab  "), 2, "пробелы по краям не считаются");
        assert_eq!(char_len("a b"), 3, "а пробел внутри считается");
        assert_eq!(char_len("   "), 0, "из одних пробелов длина ноль");
    }

    #[test]
    fn range_check_names_the_problem() {
        let short = check_len("username", "ab", 3, 20).expect("два символа это мало");
        assert_eq!((short.field.as_str(), short.code), ("username", "too_short"), "поле и код для короткого");
        let long = check_len("username", &"я".repeat(21), 3, 20).expect("21 символ это много");
        assert_eq!(long.code, "too_long", "код для длинного");
        assert!(long.message.contains("21"), "сообщение называет реальную длину: {}", long.message);
    }

    #[test]
    fn boundaries_are_inclusive() {
        assert_eq!(check_len("n", "абв", 3, 20), None, "ровно минимум проходит");
        assert_eq!(check_len("n", &"ы".repeat(20), 3, 20), None, "ровно максимум проходит");
        assert!(check_len("n", "  ab  ", 3, 20).is_some(), "после trim остаётся два символа");
    }
}
