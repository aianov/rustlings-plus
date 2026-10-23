#[derive(Debug, PartialEq)]
enum ParseError {
    BadTarget,
}

fn split_target(target: &str) -> Result<(&str, Option<&str>), ParseError> {
    if !target.starts_with('/') || target.chars().any(|c| c == ' ' || c == '#' || c.is_control()) {
        return Err(ParseError::BadTarget);
    }
    Ok(match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    })
}

fn main() {
    println!("{:?}", split_target("/search?q=rust&page=2"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_first_question_mark() {
        assert_eq!(split_target("/search?q=rust&page=2"), Ok(("/search", Some("q=rust&page=2"))));
        assert_eq!(split_target("/a?b?c"), Ok(("/a", Some("b?c"))), "делить нужно по первому '?', остальные принадлежат запросу");
    }

    #[test]
    fn missing_and_empty_query_differ() {
        assert_eq!(split_target("/a"), Ok(("/a", None)));
        assert_eq!(split_target("/a?"), Ok(("/a", Some(""))), "пустой запрос это не отсутствие запроса");
        assert_eq!(split_target("/"), Ok(("/", None)));
        assert_eq!(split_target("/?x"), Ok(("/", Some("x"))));
    }

    #[test]
    fn bad_targets() {
        for t in ["", "a/b", "*", "http://example.test/a", "/a b", "/a\tb", "/a\r\nb", "/a#frag", "/a\0"] {
            assert_eq!(split_target(t), Err(ParseError::BadTarget), "цель {t:?} должна быть отвергнута");
        }
    }

    #[test]
    fn percent_signs_are_left_alone() {
        assert_eq!(split_target("/a%20b?x=%26"), Ok(("/a%20b", Some("x=%26"))), "раскодирование это отдельный шаг, здесь % не трогаем");
    }
}
