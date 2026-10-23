#[derive(Debug, PartialEq)]
enum ParseError {
    HeadTooLarge,
    TooManyHeaders,
}

struct Limits {
    max_head: usize,
    max_headers: usize,
}

fn head_end(buf: &[u8], limits: &Limits) -> Result<Option<usize>, ParseError> {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() >= limits.max_head { Err(ParseError::HeadTooLarge) } else { Ok(None) };
    };
    let end = pos + 4;
    if end > limits.max_head {
        return Err(ParseError::HeadTooLarge);
    }
    let lines = buf[..pos].windows(2).filter(|w| w == b"\r\n").count();
    if lines > limits.max_headers {
        return Err(ParseError::TooManyHeaders);
    }
    Ok(Some(end))
}

fn main() {
    let limits = Limits { max_head: 8192, max_headers: 64 };
    println!("{:?}", head_end(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n", &limits));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_head: usize, max_headers: usize) -> Limits {
        Limits { max_head, max_headers }
    }

    const REQ: &[u8] = b"GET / HTTP/1.1\r\nHost: x\r\nA: b\r\n\r\nBODY";

    #[test]
    fn finds_the_end_and_leaves_the_rest() {
        let head = REQ.len() - 4;
        assert_eq!(head_end(REQ, &limits(1000, 10)), Ok(Some(head)), "число байтов головы включает завершающие CRLF CRLF и не включает тело");
    }

    #[test]
    fn terminator_split_across_reads() {
        let full = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        for cut in 0..full.len() {
            assert_eq!(head_end(&full[..cut], &limits(1000, 10)), Ok(None), "на {cut} байтах голова ещё не полна");
        }
        assert_eq!(head_end(full, &limits(1000, 10)), Ok(Some(full.len())));
    }

    #[test]
    fn size_limit_boundaries() {
        let full = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        let n = full.len();
        assert_eq!(head_end(full, &limits(n, 10)), Ok(Some(n)), "голова ровно в лимит помещается");
        assert_eq!(head_end(full, &limits(n - 1, 10)), Err(ParseError::HeadTooLarge), "на байт больше лимита уже не помещается");
        assert_eq!(head_end(&full[..n - 1], &limits(n, 10)), Ok(None));
    }

    #[test]
    fn endless_head_is_cut_off_early() {
        let junk = vec![b'a'; 100];
        assert_eq!(head_end(&junk, &limits(64, 10)), Err(ParseError::HeadTooLarge), "терминатора нет, а буфер уже больше лимита: ждать дальше бессмысленно");
        assert_eq!(head_end(&junk[..63], &limits(64, 10)), Ok(None));
        assert_eq!(head_end(&junk[..64], &limits(64, 10)), Err(ParseError::HeadTooLarge), "64 байта без терминатора: дописанный терминатор вылезет за лимит");
    }

    #[test]
    fn header_count_limit() {
        let mut req = b"GET / HTTP/1.1\r\n".to_vec();
        for i in 0..5 {
            req.extend_from_slice(format!("H{i}: v\r\n").as_bytes());
        }
        req.extend_from_slice(b"\r\n");
        assert_eq!(head_end(&req, &limits(10_000, 5)), Ok(Some(req.len())), "5 заголовков при лимите 5 это нормально");
        assert_eq!(head_end(&req, &limits(10_000, 4)), Err(ParseError::TooManyHeaders), "считаются заголовки, а не строка запроса");
    }

    #[test]
    fn no_headers_at_all() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\n\r\n", &limits(100, 0)), Ok(Some(18)));
    }
}
