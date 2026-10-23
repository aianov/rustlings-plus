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

fn number(pairs: &[(&str, &str)], name: &'static str, default: u32) -> Result<u32, QueryError> {
    let mut values = pairs.iter().filter(|(k, _)| *k == name);
    let Some((_, raw)) = values.next() else {
        return Ok(default);
    };
    if values.next().is_some() {
        return Err(QueryError { param: name, problem: Problem::Duplicate });
    }
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(QueryError { param: name, problem: Problem::NotANumber });
    }
    raw.parse().map_err(|_| QueryError { param: name, problem: Problem::NotANumber })
}

fn parse_page(query: &str) -> Result<Page, QueryError> {
    let pairs = query_pairs(query);
    let limit = number(&pairs, "limit", 20)?;
    if !(1..=100).contains(&limit) {
        return Err(QueryError { param: "limit", problem: Problem::OutOfRange });
    }
    let offset = number(&pairs, "offset", 0)?;
    Ok(Page { limit, offset })
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
