pub fn first_line(buf: &[u8]) -> Option<&[u8]> {
    let end = buf.iter().position(|b| *b == b'\n')?;
    let line = &buf[..end];
    Some(line.strip_suffix(b"\r").unwrap_or(line))
}

pub fn method_of(line: &[u8]) -> Option<&[u8]> {
    let end = line.iter().position(|b| *b == b' ')?;
    Some(&line[..end])
}

pub fn is_token(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b))
}

pub fn upper_ascii(bytes: &[u8]) -> Vec<u8> {
    bytes.to_ascii_uppercase()
}

fn main() {
    println!("{:?}", first_line(b"GET / HTTP/1.1\r\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_loses_its_line_ending() {
        assert_eq!(first_line(b"GET /x HTTP/1.1\r\nHost: a\r\n"), Some(&b"GET /x HTTP/1.1"[..]));
        assert_eq!(first_line(b"bare newline\nrest"), Some(&b"bare newline"[..]));
        assert_eq!(first_line(b"\r\n"), Some(&b""[..]));
    }

    #[test]
    fn an_unfinished_line_is_not_a_line() {
        assert_eq!(first_line(b"GET /x HT"), None, "перевода строки ещё нет: ждём продолжения");
        assert_eq!(first_line(b""), None);
    }

    #[test]
    fn bytes_outside_utf8_are_fine() {
        let buf = b"GET /\xff\xfe HTTP/1.1\r\n";
        let line = first_line(buf).expect("строка есть");
        assert_eq!(line.len(), 16);
        assert_eq!(method_of(line), Some(&b"GET"[..]));
    }

    #[test]
    fn the_method_ends_at_the_first_space() {
        assert_eq!(method_of(b"POST /a b"), Some(&b"POST"[..]));
        assert_eq!(method_of(b" /leading"), Some(&b""[..]));
        assert_eq!(method_of(b"NOSPACE"), None);
    }

    #[test]
    fn tokens_are_ascii_symbols_only() {
        assert!(is_token(b"Content-Length"));
        assert!(is_token(b"X_Custom.v2"));
        assert!(!is_token(b""));
        assert!(!is_token(b"Bad Header"));
        assert!(!is_token(b"Name:"));
        assert!(!is_token("Ключ".as_bytes()), "байты вне ASCII не годятся для имени заголовка");
    }

    #[test]
    fn uppercase_touches_only_ascii_letters() {
        assert_eq!(upper_ascii(b"get /a-1"), b"GET /A-1".to_vec());
        assert_eq!(upper_ascii("ab é".as_bytes()), "AB é".as_bytes().to_vec(), "байты буквы é остаются прежними");
        assert_eq!(upper_ascii(&[0xff, b'z']), vec![0xff, b'Z']);
    }
}
