use std::str;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Method {
    Get,
    Post,
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
    BadHeader,
    BadContentLength,
}

#[derive(Debug, PartialEq)]
struct Request {
    method: Method,
    target: String,
    version: Version,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum Parsed {
    Partial,
    Complete { request: Request, consumed: usize },
}

fn parse_request_line(line: &str) -> Result<(Method, String, Version), ParseError> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(ParseError::BadRequestLine);
    }
    let method = match parts[0] {
        "GET" => Method::Get,
        "POST" => Method::Post,
        _ => return Err(ParseError::BadMethod),
    };
    let version = match parts[2] {
        "HTTP/1.0" => Version::Http10,
        "HTTP/1.1" => Version::Http11,
        _ => return Err(ParseError::BadVersion),
    };
    Ok((method, parts[1].to_string(), version))
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

fn parse_header_line(line: &str) -> Result<(String, String), ParseError> {
    let (name, value) = line.split_once(':').ok_or(ParseError::BadHeader)?;
    if name.is_empty() || !name.chars().all(is_token_char) {
        return Err(ParseError::BadHeader);
    }
    let value = value.trim_matches(|c| c == ' ' || c == '\t');
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ParseError::BadHeader);
    }
    Ok((name.to_string(), value.to_string()))
}

fn parse_request(buf: &[u8]) -> Result<Parsed, ParseError> {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Ok(Parsed::Partial);
    };
    let head_len = pos + 4;
    let head = str::from_utf8(&buf[..pos]).map_err(|_| ParseError::BadHeader)?;
    let mut lines = head.split("\r\n");
    let (method, target, version) = parse_request_line(lines.next().unwrap_or(""))?;
    let mut headers = Vec::new();
    for line in lines {
        headers.push(parse_header_line(line)?);
    }
    let length = match headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-length")) {
        Some((_, v)) => v.parse::<usize>().map_err(|_| ParseError::BadContentLength)?,
        None => 0,
    };
    let total = head_len + length;
    if buf.len() < total {
        return Ok(Parsed::Partial);
    }
    let request = Request { method, target, version, headers, body: buf[head_len..total].to_vec() };
    Ok(Parsed::Complete { request, consumed: total })
}

fn main() {
    println!("{:?}", parse_request(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GET: &[u8] = b"GET /books?id=7 HTTP/1.1\r\nHost: shop.test\r\nAccept: */*\r\n\r\n";
    const POST: &[u8] = b"POST /books HTTP/1.1\r\nHost: shop.test\r\nContent-Length: 11\r\n\r\nline1\r\n\r\nok";

    fn complete(buf: &[u8]) -> (Request, usize) {
        match parse_request(buf) {
            Ok(Parsed::Complete { request, consumed }) => (request, consumed),
            other => panic!("ожидался готовый запрос, получено {other:?}"),
        }
    }

    #[test]
    fn simple_get() {
        let (req, used) = complete(GET);
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.target, "/books?id=7");
        assert_eq!(req.version, Version::Http11);
        assert_eq!(req.headers, vec![("Host".to_string(), "shop.test".to_string()), ("Accept".to_string(), "*/*".to_string())]);
        assert!(req.body.is_empty());
        assert_eq!(used, GET.len());
    }

    #[test]
    fn body_by_content_length_even_with_blank_line_inside() {
        let (req, used) = complete(POST);
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.body, b"line1\r\n\r\nok", "в теле нашлась пустая строка: это уже не конец заголовков");
        assert_eq!(used, POST.len());
    }

    #[test]
    fn every_proper_prefix_is_partial() {
        for full in [GET, POST] {
            for cut in 0..full.len() {
                assert_eq!(parse_request(&full[..cut]), Ok(Parsed::Partial), "на {cut} байтах из {} запрос ещё не готов", full.len());
            }
        }
    }

    #[test]
    fn pipelined_requests() {
        let mut data = GET.to_vec();
        data.extend_from_slice(POST);
        data.extend_from_slice(b"GET /third HTTP/1.1\r\nHo");
        let (first, used1) = complete(&data);
        assert_eq!(first.target, "/books?id=7");
        assert_eq!(used1, GET.len(), "consumed это байты только первого запроса");
        let (second, used2) = complete(&data[used1..]);
        assert_eq!(second.method, Method::Post);
        assert_eq!(used2, POST.len());
        assert_eq!(parse_request(&data[used1 + used2..]), Ok(Parsed::Partial), "третий запрос обрезан");
    }

    #[test]
    fn errors_appear_once_the_head_is_complete() {
        assert_eq!(parse_request(b"BREW / HTTP/1.1\r\n\r\n"), Err(ParseError::BadMethod));
        assert_eq!(parse_request(b"GET / HTTP/9\r\n\r\n"), Err(ParseError::BadVersion));
        assert_eq!(parse_request(b"GET / HTTP/1.1\r\nBad Header: x\r\n\r\n"), Err(ParseError::BadHeader));
        assert_eq!(parse_request(b"POST / HTTP/1.1\r\nContent-Length: abc\r\n\r\n"), Err(ParseError::BadContentLength), "ошибка длины видна сразу, тело ждать не нужно");
        assert_eq!(parse_request(b"\r\n\r\n"), Err(ParseError::BadRequestLine));
    }

    #[test]
    fn content_length_name_is_case_insensitive() {
        let (req, _) = complete(b"POST / HTTP/1.1\r\ncontent-length: 2\r\n\r\nhi");
        assert_eq!(req.body, b"hi");
    }
}
