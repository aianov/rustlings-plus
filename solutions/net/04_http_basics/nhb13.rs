#[derive(Debug, PartialEq, Clone, Copy)]
enum Version {
    Http10,
    Http11,
}

#[derive(Debug, PartialEq)]
enum ParseError {
    MissingHost,
    DuplicateHost,
}

fn check_host(version: Version, headers: &[(&str, &str)]) -> Result<(), ParseError> {
    if version == Version::Http10 {
        return Ok(());
    }
    let hosts: Vec<&str> = headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case("host")).map(|(_, v)| v.trim()).collect();
    match hosts.as_slice() {
        [] => Err(ParseError::MissingHost),
        [""] => Err(ParseError::MissingHost),
        [_] => Ok(()),
        _ => Err(ParseError::DuplicateHost),
    }
}

fn keep_alive(version: Version, headers: &[(&str, &str)]) -> bool {
    let mut close = false;
    let mut keep = false;
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("connection") {
            continue;
        }
        for token in value.split(',') {
            let token = token.trim();
            close |= token.eq_ignore_ascii_case("close");
            keep |= token.eq_ignore_ascii_case("keep-alive");
        }
    }
    if close {
        return false;
    }
    keep || version == Version::Http11
}

fn main() {
    println!("{}", keep_alive(Version::Http11, &[("Connection", "close")]));
    println!("{:?}", check_host(Version::Http11, &[]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_rules() {
        assert_eq!(check_host(Version::Http11, &[("Host", "a.test")]), Ok(()));
        assert_eq!(check_host(Version::Http11, &[("hOsT", "a.test:8080")]), Ok(()), "имя заголовка без учёта регистра");
        assert_eq!(check_host(Version::Http11, &[("Accept", "*/*")]), Err(ParseError::MissingHost));
        assert_eq!(check_host(Version::Http11, &[("Host", "")]), Err(ParseError::MissingHost), "пустой Host не лучше отсутствующего");
        assert_eq!(check_host(Version::Http11, &[("Host", "a.test"), ("host", "b.test")]), Err(ParseError::DuplicateHost), "два Host: прокси и сервер выберут разные");
        assert_eq!(check_host(Version::Http10, &[]), Ok(()), "в HTTP/1.0 Host не обязателен");
    }

    #[test]
    fn defaults_by_version() {
        assert!(keep_alive(Version::Http11, &[]), "HTTP/1.1 держит соединение по умолчанию");
        assert!(!keep_alive(Version::Http10, &[]), "HTTP/1.0 по умолчанию закрывает");
    }

    #[test]
    fn explicit_connection_header() {
        assert!(!keep_alive(Version::Http11, &[("Connection", "close")]));
        assert!(!keep_alive(Version::Http11, &[("connection", "CLOSE")]), "значения тоже без учёта регистра");
        assert!(keep_alive(Version::Http10, &[("Connection", "keep-alive")]), "HTTP/1.0 с keep-alive остаётся");
        assert!(keep_alive(Version::Http10, &[("Connection", "Keep-Alive")]));
    }

    #[test]
    fn lists_and_repeated_headers() {
        assert!(!keep_alive(Version::Http11, &[("Connection", "keep-alive, close")]), "close в списке побеждает");
        assert!(!keep_alive(Version::Http11, &[("Connection", "Upgrade , Close")]), "токены надо разбирать по запятой и обрезать пробелы");
        assert!(!keep_alive(Version::Http11, &[("Connection", "keep-alive"), ("Connection", "close")]), "значения из нескольких строк объединяются");
        assert!(keep_alive(Version::Http11, &[("Connection", "Upgrade")]), "чужие токены решения не меняют");
        assert!(keep_alive(Version::Http11, &[("Connection", "closed-form")]), "токен close должен совпасть целиком, а не быть подстрокой");
    }
}
