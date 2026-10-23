// Фронтовой прокси и бэкенд читают одни и те же заголовки по-разному, и злоумышленник прячет второй
// запрос внутри первого: это называется контрабандой запросов (request smuggling). Функция ниже
// определяет длину тела и ошибается ровно там, где ошибаются настоящие серверы: в неоднозначных
// случаях она делает вид, что всё в порядке.
// TODO: Почини content_length. Нет заголовка, это Ok(None). Значение только из цифр (без знака, пробелов,
// "0x"), и оно помещается в usize: иначе BadContentLength. Неоднозначность это Smuggling: разные значения
// в нескольких заголовках или в списке через запятую, либо Content-Length вместе с Transfer-Encoding.

#[derive(Debug, PartialEq)]
enum ParseError {
    BadContentLength,
    Smuggling,
}

fn content_length(headers: &[(&str, &str)]) -> Result<Option<usize>, ParseError> {
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            return Ok(value.trim().parse().ok());
        }
    }
    Ok(None)
}

fn main() {
    println!("{:?}", content_length(&[("Content-Length", "42")]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_cases() {
        assert_eq!(content_length(&[("Host", "x")]), Ok(None));
        assert_eq!(content_length(&[("content-length", "42")]), Ok(Some(42)));
        assert_eq!(content_length(&[("Content-Length", " 7\t")]), Ok(Some(7)), "пробелы по краям значения допустимы");
        assert_eq!(content_length(&[("Content-Length", "0")]), Ok(Some(0)));
        assert_eq!(content_length(&[("Transfer-Encoding", "chunked")]), Ok(None), "без Content-Length длину определяет chunked, это другая задача");
    }

    #[test]
    fn malformed_numbers() {
        for v in ["+5", "-1", "0x10", "5 5", "", " ", "1e3", "abc", "5.0", "99999999999999999999999"] {
            assert_eq!(content_length(&[("Content-Length", v)]), Err(ParseError::BadContentLength), "значение {v:?} не число из цифр: молча принимать нельзя");
        }
    }

    #[test]
    fn conflicting_lengths_are_smuggling() {
        assert_eq!(content_length(&[("Content-Length", "5"), ("Content-Length", "6")]), Err(ParseError::Smuggling), "два разных Content-Length: один сервер возьмёт первый, другой второй");
        assert_eq!(content_length(&[("Content-Length", "5, 6")]), Err(ParseError::Smuggling), "список с разными значениями то же самое");
        assert_eq!(content_length(&[("content-length", "3"), ("Content-Length", "4")]), Err(ParseError::Smuggling), "регистр имени тут ни при чём");
    }

    #[test]
    fn identical_duplicates_are_harmless() {
        assert_eq!(content_length(&[("Content-Length", "5"), ("content-length", "5")]), Ok(Some(5)));
        assert_eq!(content_length(&[("Content-Length", "5, 5")]), Ok(Some(5)));
    }

    #[test]
    fn length_together_with_transfer_encoding() {
        assert_eq!(content_length(&[("Content-Length", "4"), ("Transfer-Encoding", "chunked")]), Err(ParseError::Smuggling), "оба способа задать тело сразу: классическая контрабанда");
        assert_eq!(content_length(&[("transfer-encoding", "chunked"), ("Content-Length", "4")]), Err(ParseError::Smuggling), "порядок заголовков значения не имеет");
    }
}
