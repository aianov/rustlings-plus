#[derive(Debug, PartialEq)]
enum ParseError {
    BadEscape,
}

fn percent_decode(s: &str, plus_as_space: bool) -> Result<String, ParseError> {
    let bytes = s.as_bytes();
    let hex = |b: Option<&u8>| b.and_then(|b| (*b as char).to_digit(16)).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => match (hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) {
                (Some(hi), Some(lo)) => {
                    out.push(hi << 4 | lo);
                    i += 3;
                }
                _ => return Err(ParseError::BadEscape),
            },
            b'+' if plus_as_space => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ParseError::BadEscape)
}

fn parse_query(query: &str) -> Result<Vec<(String, String)>, ParseError> {
    let mut out = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.push((percent_decode(key, true)?, percent_decode(value, true)?));
    }
    Ok(out)
}

fn main() {
    println!("{:?}", parse_query("q=a%26b&lang=ru"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn simple_pairs() {
        assert_eq!(parse_query("a=1&b=2"), Ok(pairs(&[("a", "1"), ("b", "2")])));
        assert_eq!(parse_query("flag"), Ok(pairs(&[("flag", "")])));
        assert_eq!(parse_query("a=1&&b=2&"), Ok(pairs(&[("a", "1"), ("b", "2")])));
        assert_eq!(parse_query(""), Ok(vec![]));
        assert_eq!(parse_query("k="), Ok(pairs(&[("k", "")])));
    }

    #[test]
    fn escaped_delimiters_stay_inside_the_value() {
        assert_eq!(parse_query("q=a%26b&lang=ru"), Ok(pairs(&[("q", "a&b"), ("lang", "ru")])), "%26 это амперсанд внутри значения, а не разделитель параметров");
        assert_eq!(parse_query("x=1%3D2"), Ok(pairs(&[("x", "1=2")])), "%3D это знак равенства внутри значения");
        assert_eq!(parse_query("a%3Db=1"), Ok(pairs(&[("a=b", "1")])), "и внутри ключа тоже");
    }

    #[test]
    fn plus_and_unicode() {
        assert_eq!(parse_query("q=hello+world"), Ok(pairs(&[("q", "hello world")])));
        assert_eq!(parse_query("q=a%2Bb"), Ok(pairs(&[("q", "a+b")])));
        assert_eq!(parse_query("name=%D0%AF"), Ok(pairs(&[("name", "Я")])));
    }

    #[test]
    fn value_may_contain_equals() {
        assert_eq!(parse_query("expr=a=b"), Ok(pairs(&[("expr", "a=b")])));
    }

    #[test]
    fn bad_escape_is_reported() {
        assert_eq!(parse_query("a=%zz"), Err(ParseError::BadEscape));
    }
}
