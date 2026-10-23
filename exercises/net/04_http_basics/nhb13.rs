// После ответа соединение либо закрывают, либо оставляют для следующего запроса (keep-alive). Решение
// зависит от версии протокола и заголовка Connection, у которого может быть список значений,
// записанных в любом регистре и разбросанных по нескольким строкам. А ещё HTTP/1.1 требует
// ровно один Host: без него сервер не знает, какой из его сайтов запрошен.
// TODO: Допиши check_host (для 1.1 нужен ровно один непустой Host, для 1.0 не обязателен) и keep_alive
// (1.1 по умолчанию остаётся, 1.0 по умолчанию закрывает; "close" побеждает всё, "keep-alive" включает для 1.0).

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
    todo!()
}

fn keep_alive(version: Version, headers: &[(&str, &str)]) -> bool {
    todo!()
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
