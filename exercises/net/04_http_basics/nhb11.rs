// Когда размер тела заранее неизвестен (ответ генерируется на лету), сервер шлёт его кусками:
// "размер в hex, CRLF, данные, CRLF", а в конце кусок нулевого размера и пустая строка. Данные могут
// прийти на любой границе, поэтому декодер видит только префикс сообщения и обязан отвечать «пока
// мало», а не ошибкой.
// TODO: Напиши decode_chunked. Некорректный размер (не hex, длиннее 16 цифр), отсутствие CRLF после
// данных, сумма размеров больше max_body это ошибки сразу, как только заметны. После размера может идти
// расширение через ';' (его игнорируй), а сама строка размера не может быть длиннее пары сотен байтов:
// без этого лимита поток без CRLF копился бы бесконечно. После "0" могут идти трейлеры (строки
// заголовков) до пустой строки: пропусти их.

#[derive(Debug, PartialEq)]
enum ParseError {
    BadChunk,
    BodyTooLarge,
}

fn decode_chunked(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    todo!()
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
