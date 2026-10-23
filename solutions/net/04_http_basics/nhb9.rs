#[derive(Debug, PartialEq)]
enum ParseError {
    BodyTooLarge,
}

#[derive(Debug, PartialEq)]
enum Body<'a> {
    Partial { missing: usize },
    Complete(&'a [u8]),
}

fn take_body(buf: &[u8], content_length: usize, max_body: usize) -> Result<Body<'_>, ParseError> {
    if content_length > max_body {
        return Err(ParseError::BodyTooLarge);
    }
    if buf.len() < content_length {
        return Ok(Body::Partial { missing: content_length - buf.len() });
    }
    Ok(Body::Complete(&buf[..content_length]))
}

fn main() {
    println!("{:?}", take_body(b"hello", 5, 1024));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_partial() {
        assert_eq!(take_body(b"hello", 5, 100), Ok(Body::Complete(b"hello")));
        assert_eq!(take_body(b"hel", 5, 100), Ok(Body::Partial { missing: 2 }));
        assert_eq!(take_body(b"", 5, 100), Ok(Body::Partial { missing: 5 }));
    }

    #[test]
    fn the_rest_belongs_to_the_next_request() {
        assert_eq!(take_body(b"helloGET / HTTP/1.1\r\n", 5, 100), Ok(Body::Complete(b"hello")), "тело ровно 5 байтов, остаток это следующий запрос");
    }

    #[test]
    fn empty_body() {
        assert_eq!(take_body(b"", 0, 100), Ok(Body::Complete(b"")));
        assert_eq!(take_body(b"next", 0, 100), Ok(Body::Complete(b"")), "при длине 0 ни одного байта не забираем");
    }

    #[test]
    fn too_large_is_rejected_before_any_data() {
        assert_eq!(take_body(b"", 101, 100), Err(ParseError::BodyTooLarge), "отвергать надо по заявленной длине, не ожидая данных");
        assert_eq!(take_body(b"", usize::MAX, 100), Err(ParseError::BodyTooLarge), "огромная длина не должна вызывать переполнения или выделения памяти");
        assert_eq!(take_body(b"", 100, 100), Ok(Body::Partial { missing: 100 }), "ровно в лимит можно");
    }

    #[test]
    fn binary_bodies_are_untouched() {
        let body = [0u8, 255, 13, 10, 13, 10, 0];
        assert_eq!(take_body(&body, 7, 100), Ok(Body::Complete(&body[..])), "в теле бывают любые байты, включая CRLF CRLF и нули");
    }
}
