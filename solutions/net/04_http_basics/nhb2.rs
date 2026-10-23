use std::str::FromStr;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Version {
    Http10,
    Http11,
}

#[derive(Debug, PartialEq)]
#[allow(clippy::enum_variant_names)]
enum ParseError {
    BadRequestLine,
    BadMethod,
    BadVersion,
}

impl FromStr for Method {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Method, ParseError> {
        match s {
            "GET" => Ok(Method::Get),
            "HEAD" => Ok(Method::Head),
            "POST" => Ok(Method::Post),
            "PUT" => Ok(Method::Put),
            "DELETE" => Ok(Method::Delete),
            _ => Err(ParseError::BadMethod),
        }
    }
}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Version, ParseError> {
        match s {
            "HTTP/1.0" => Ok(Version::Http10),
            "HTTP/1.1" => Ok(Version::Http11),
            _ => Err(ParseError::BadVersion),
        }
    }
}

fn parse_request_line(line: &str) -> Result<(Method, String, Version), ParseError> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(ParseError::BadRequestLine);
    }
    Ok((parts[0].parse()?, parts[1].to_string(), parts[2].parse()?))
}

fn main() {
    println!("{:?}", parse_request_line("GET /books?id=7 HTTP/1.1"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_lines() {
        assert_eq!(parse_request_line("GET / HTTP/1.1"), Ok((Method::Get, "/".to_string(), Version::Http11)));
        assert_eq!(parse_request_line("POST /books?id=7 HTTP/1.0"), Ok((Method::Post, "/books?id=7".to_string(), Version::Http10)));
        assert_eq!(parse_request_line("HEAD /a HTTP/1.1").map(|r| r.0), Ok(Method::Head));
        assert_eq!(parse_request_line("PUT /a HTTP/1.1").map(|r| r.0), Ok(Method::Put));
        assert_eq!(parse_request_line("DELETE /a HTTP/1.1").map(|r| r.0), Ok(Method::Delete));
    }

    #[test]
    fn methods_are_case_sensitive_and_known() {
        assert_eq!(parse_request_line("get / HTTP/1.1").unwrap_err(), ParseError::BadMethod, "имена методов чувствительны к регистру");
        assert_eq!(parse_request_line("BREW /pot HTTP/1.1").unwrap_err(), ParseError::BadMethod, "неизвестный метод это BadMethod");
    }

    #[test]
    fn versions_are_strict() {
        for v in ["HTTP/2.0", "http/1.1", "HTTP/1.2", "HTTP/1", "1.1"] {
            assert_eq!(parse_request_line(&format!("GET / {v}")).unwrap_err(), ParseError::BadVersion, "версия {v:?} не поддерживается");
        }
    }

    #[test]
    fn shape_errors() {
        for line in ["GET /", "GET", "", "GET / HTTP/1.1 extra", "GET  / HTTP/1.1", " GET / HTTP/1.1", "GET / HTTP/1.1 "] {
            assert_eq!(parse_request_line(line).unwrap_err(), ParseError::BadRequestLine, "строка {line:?} имеет неверную форму: ровно три части, разделённые одним пробелом");
        }
    }

    #[test]
    fn version_from_str_alone() {
        assert_eq!("HTTP/1.1".parse::<Version>(), Ok(Version::Http11));
        assert_eq!("nope".parse::<Version>(), Err(ParseError::BadVersion));
        assert_eq!("nope".parse::<Method>(), Err(ParseError::BadMethod));
    }
}
