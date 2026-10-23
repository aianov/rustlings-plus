// В адресах нельзя прямо писать пробел, амперсанд или русские буквы, их кодируют как %XX, где XX это
// байт в шестнадцатеричном виде: пробел это %20, "é" это %C3%A9 (два байта UTF-8). В запросе формы
// пробел ещё может быть плюсом. Нужен декодер, который не доверяет входу.
// TODO: Напиши с нуля percent_decode. Некорректная последовательность (не hex, обрыв) или байты, не
// складывающиеся в UTF-8, это BadEscape. Плюс означает пробел только при plus_as_space.

#[derive(Debug, PartialEq)]
enum ParseError {
    BadEscape,
}

fn percent_decode(s: &str, plus_as_space: bool) -> Result<String, ParseError> {
    todo!()
}

fn main() {
    println!("{:?}", percent_decode("caf%C3%A9+au+lait", true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_escaped() {
        assert_eq!(percent_decode("abc", false), Ok("abc".to_string()));
        assert_eq!(percent_decode("a%20b", false), Ok("a b".to_string()));
        assert_eq!(percent_decode("%2F%3f", false), Ok("/?".to_string()), "шестнадцатеричные цифры можно писать в любом регистре");
        assert_eq!(percent_decode("", false), Ok(String::new()));
    }

    #[test]
    fn plus_depends_on_the_mode() {
        assert_eq!(percent_decode("a+b", false), Ok("a+b".to_string()), "в пути плюс это обычный символ");
        assert_eq!(percent_decode("a+b", true), Ok("a b".to_string()));
        assert_eq!(percent_decode("a%2Bb", true), Ok("a+b".to_string()), "%2B это настоящий плюс, он пробелом не становится");
    }

    #[test]
    fn multibyte_utf8() {
        assert_eq!(percent_decode("caf%C3%A9", false), Ok("café".to_string()));
        assert_eq!(percent_decode("%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82", false), Ok("привет".to_string()));
    }

    #[test]
    fn broken_escapes_are_errors() {
        for s in ["%zz", "%4", "%", "100%", "%G1", "a%2", "%%41", "%4 "] {
            assert_eq!(percent_decode(s, false), Err(ParseError::BadEscape), "{s:?} это неверная последовательность");
        }
    }

    #[test]
    fn bytes_that_are_not_utf8_are_errors() {
        for s in ["%ff", "%C3", "%e2%82", "%80"] {
            assert_eq!(percent_decode(s, false), Err(ParseError::BadEscape), "{s:?} не складывается в UTF-8: строка Rust не может хранить такие байты");
        }
    }
}
