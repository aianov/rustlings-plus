#[derive(Debug, PartialEq)]
enum ParseError {
    BadChunk,
    BodyTooLarge,
}

fn decode_chunked(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    fn crlf(buf: &[u8]) -> Option<usize> {
        buf.windows(2).position(|w| w == b"\r\n")
    }

    let mut pos = 0;
    let mut body = Vec::new();
    loop {
        let rest = &buf[pos..];
        let Some(end) = crlf(rest) else {
            return if rest.len() > 64 { Err(ParseError::BadChunk) } else { Ok(None) };
        };
        let line = std::str::from_utf8(&rest[..end]).map_err(|_| ParseError::BadChunk)?;
        let digits = line.split(';').next().unwrap_or("");
        if digits.is_empty() || digits.len() > 16 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseError::BadChunk);
        }
        let size = usize::from_str_radix(digits, 16).map_err(|_| ParseError::BadChunk)?;
        pos += end + 2;
        if size == 0 {
            loop {
                let Some(end) = crlf(&buf[pos..]) else { return Ok(None) };
                pos += end + 2;
                if end == 0 {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if body.len().saturating_add(size) > max_body {
            return Err(ParseError::BodyTooLarge);
        }
        let available = buf.len() - pos;
        if available < size.saturating_add(2) {
            if available > size && buf[pos + size] != b'\r' {
                return Err(ParseError::BadChunk);
            }
            return Ok(None);
        }
        if &buf[pos + size..pos + size + 2] != b"\r\n" {
            return Err(ParseError::BadChunk);
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        pos += size + 2;
    }
}

fn main() {
    println!("{:?}", decode_chunked(b"5\r\nhello\r\n0\r\n\r\n", 1024));
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIKI: &[u8] = b"4\r\nWiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n";

    #[test]
    fn decodes_and_reports_consumed() {
        let (body, used) = decode_chunked(WIKI, 1000).unwrap().expect("полное сообщение");
        assert_eq!(body, b"Wikipedia in\r\n\r\nchunks.", "данные куска могут содержать CRLF, полагаться надо на размер");
        assert_eq!(used, WIKI.len());
    }

    #[test]
    fn leftover_is_not_consumed() {
        let mut data = WIKI.to_vec();
        data.extend_from_slice(b"GET / HTTP/1.1\r\n");
        let (_, used) = decode_chunked(&data, 1000).unwrap().unwrap();
        assert_eq!(used, WIKI.len(), "следующий запрос в consumed входить не должен");
    }

    #[test]
    fn every_prefix_is_just_incomplete() {
        for cut in 0..WIKI.len() {
            assert_eq!(decode_chunked(&WIKI[..cut], 1000), Ok(None), "префикс из {cut} байтов корректен, но неполон: это не ошибка");
        }
    }

    #[test]
    fn extensions_uppercase_hex_and_trailers() {
        let data = b"A;name=val\r\n0123456789\r\n0\r\nX-Sum: 1\r\nX-Other: 2\r\n\r\n";
        let (body, used) = decode_chunked(data, 100).unwrap().unwrap();
        assert_eq!(body, b"0123456789");
        assert_eq!(used, data.len(), "трейлеры входят в consumed");
        assert_eq!(decode_chunked(&data[..data.len() - 2], 100), Ok(None), "пустой строки после трейлеров ещё нет");
    }

    #[test]
    fn empty_body() {
        assert_eq!(decode_chunked(b"0\r\n\r\n", 10), Ok(Some((vec![], 5))));
    }

    #[test]
    fn malformed_chunks() {
        for data in [&b"zz\r\nab\r\n0\r\n\r\n"[..], b"\r\n", b"-5\r\nabcde\r\n", b"5\r\nhelloXX0\r\n\r\n", b"FFFFFFFFFFFFFFFFFF\r\n", b"5 \r\nhello\r\n"] {
            assert_eq!(decode_chunked(data, 1_000_000), Err(ParseError::BadChunk), "{:?} должен быть отвергнут", String::from_utf8_lossy(data));
        }
        assert_eq!(decode_chunked(&[b'a'; 10_000], 1000), Err(ParseError::BadChunk), "строка размера без конца не должна копиться бесконечно");
    }

    #[test]
    fn size_limit_checked_before_the_data_arrives() {
        assert_eq!(decode_chunked(b"FFFFFFF\r\n", 1000), Err(ParseError::BodyTooLarge), "заявленный кусок больше лимита: ждать его данные незачем");
        assert_eq!(decode_chunked(b"7FFFFFFFFFFFFFFF\r\n", 1000), Err(ParseError::BodyTooLarge), "огромный размер не должен переполнять арифметику");
        assert_eq!(decode_chunked(b"3\r\nabc\r\n3\r\nabc\r\n0\r\n\r\n", 5), Err(ParseError::BodyTooLarge), "лимит на сумму кусков, а не на каждый");
    }
}
