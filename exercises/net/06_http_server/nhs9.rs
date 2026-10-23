// Список постов принимает "?limit=10&offset=30". Ленивый обработчик делает parse().unwrap() и падает на
// "?limit=abc"; чуть менее ленивый молча подставит ноль, и клиент никогда не узнает, что опечатался.
// Хороший сервер говорит, какой именно параметр плох и чем. query_pairs, который режет строку, уже есть.
// TODO: Напиши parse_page. Умолчания: limit 20, offset 0. limit от 1 до 100, offset любое u32. Только цифры
// (знак и пробелы это не число), повтор параметра это ошибка, чужие параметры игнорируются. Если плохи оба,
// сообщи о limit.

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
struct Page {
    limit: u32,
    offset: u32,
}

#[derive(Debug, PartialEq)]
enum Problem {
    Duplicate,
    NotANumber,
    OutOfRange,
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
struct QueryError {
    param: &'static str,
    problem: Problem,
}

fn query_pairs(query: &str) -> Vec<(&str, &str)> {
    query.split('&').filter(|p| !p.is_empty()).map(|p| p.split_once('=').unwrap_or((p, ""))).collect()
}

fn parse_page(query: &str) -> Result<Page, QueryError> {
    todo!()
}

fn main() {
    println!("{:?}", parse_page("limit=10&offset=30"));
    println!("{:?}", parse_page("limit=abc"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(query: &str) -> (&'static str, Problem) {
        let e = parse_page(query).expect_err(&format!("{query:?} должен быть отвергнут"));
        (e.param, e.problem)
    }

    #[test]
    fn defaults_and_values() {
        assert_eq!(parse_page(""), Ok(Page { limit: 20, offset: 0 }));
        assert_eq!(parse_page("offset=5"), Ok(Page { limit: 20, offset: 5 }));
        assert_eq!(parse_page("limit=100&offset=4294967295"), Ok(Page { limit: 100, offset: u32::MAX }));
        assert_eq!(parse_page("x=1&limit=1&&y&offset=0"), Ok(Page { limit: 1, offset: 0 }), "чужие параметры и пустые куски не мешают");
    }

    #[test]
    fn rejects_non_numbers() {
        for bad in ["limit=abc", "limit=", "limit=-1", "limit=+5", "limit=1.5", "limit=%205", "limit=99999999999"] {
            assert_eq!(err(bad), ("limit", Problem::NotANumber), "{bad}");
        }
        assert_eq!(err("offset=-1"), ("offset", Problem::NotANumber));
        assert_eq!(err("limit"), ("limit", Problem::NotANumber), "параметр без '=' это пустое значение");
    }

    #[test]
    fn range_and_duplicates() {
        assert_eq!(err("limit=0"), ("limit", Problem::OutOfRange));
        assert_eq!(err("limit=101"), ("limit", Problem::OutOfRange));
        assert_eq!(err("limit=5&limit=5"), ("limit", Problem::Duplicate), "даже одинаковые повторы это неоднозначность");
        assert_eq!(err("offset=1&offset=2"), ("offset", Problem::Duplicate));
        assert_eq!(err("limit=x&offset=y"), ("limit", Problem::NotANumber), "из двух ошибок сообщаем про limit");
    }
}
