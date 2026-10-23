// Ответ приходит кусками, как и запрос: голова может быть разрезана где угодно. Нужен разбор, который на
// неполной голове молча говорит «ещё не всё», а на полной возвращает статус, заголовки и сколько байтов она заняла.
// Остаток буфера после головы уже принадлежит телу, и трогать его нельзя.
// TODO: Напиши parse_head: Ok(None), пока нет пустой строки; Ok(Some(Head)) с len до конца пустой строки включительно.
// Строка статуса: HTTP/1.0 или HTTP/1.1, трёхзначный код от 100, причина может быть пустой или отсутствовать.
// Заголовки: имя из токена без пробелов, значение без краевых пробелов, продолжения строк (обs-fold) это ошибка.
// Голова больше MAX_HEAD это TooLarge, даже если конца ещё не видно.

type Headers = Vec<(String, String)>;

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

#[derive(Debug, Clone, PartialEq)]
struct Head {
    minor: u8,
    status: u16,
    reason: String,
    headers: Headers,
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

const MAX_HEAD: usize = 16 * 1024;

#[allow(dead_code)]
impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.all("connection") {
            for token in value.split(',') {
                close |= token.trim().eq_ignore_ascii_case("close");
                keep |= token.trim().eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.minor == 1)
    }
}

fn parse_head(buf: &[u8]) -> Result<Option<Head>, HeadError> {
    todo!()
}

fn main() {
    let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi";
    println!("{:?}", parse_head(wire));
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nX-Trace:   abc  \r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n";

    #[test]
    fn full_head() {
        let head = parse_head(FULL).unwrap().expect("голова целая, её нужно разобрать");
        assert_eq!((head.minor, head.status, head.reason.as_str()), (1, 404, "Not Found"));
        assert_eq!(head.len, FULL.len());
        assert_eq!(head.header("content-type"), Some("text/plain"), "имя заголовка ищется без учёта регистра");
        assert_eq!(head.header("X-Trace"), Some("abc"), "пробелы вокруг значения срезаются");
        assert_eq!(head.all("set-cookie"), vec!["a=1", "b=2"], "повторяющиеся заголовки не склеиваются и не теряются");
        assert_eq!(head.headers[0].0, "Content-Type", "имена хранятся как пришли");
    }

    #[test]
    fn len_stops_at_the_head() {
        let mut wire = FULL.to_vec();
        wire.extend_from_slice(b"body bytes here");
        let head = parse_head(&wire).unwrap().unwrap();
        assert_eq!(head.len, FULL.len(), "len это длина головы вместе с пустой строкой, тело не считается");
    }

    #[test]
    fn every_prefix_is_just_not_ready() {
        for cut in 0..FULL.len() {
            assert_eq!(parse_head(&FULL[..cut]), Ok(None), "на {cut} байтах голова ещё не пришла: это не ошибка");
        }
        assert!(parse_head(FULL).unwrap().is_some());
    }

    #[test]
    fn odd_but_valid_status_lines() {
        let head = parse_head(b"HTTP/1.1 200 \r\n\r\n").unwrap().unwrap();
        assert_eq!((head.status, head.reason.as_str()), (200, ""));
        let head = parse_head(b"HTTP/1.1 204\r\n\r\n").unwrap().unwrap();
        assert_eq!((head.status, head.reason.as_str()), (204, ""), "причина может отсутствовать совсем");
        let head = parse_head(b"HTTP/1.0 503 Service Is Gone Fishing\r\n\r\n").unwrap().unwrap();
        assert_eq!((head.minor, head.status, head.reason.as_str()), (0, 503, "Service Is Gone Fishing"), "в причине бывают пробелы");
        let head = parse_head(b"HTTP/1.1 100 Continue\r\n\r\n").unwrap().unwrap();
        assert_eq!(head.status, 100);
        let head = parse_head(b"HTTP/1.1 200 OK\r\nEmpty:\r\nTab:\tvalue\t\r\n\r\n").unwrap().unwrap();
        assert_eq!(head.header("empty"), Some(""));
        assert_eq!(head.header("tab"), Some("value"));
    }

    #[test]
    fn broken_status_lines() {
        assert_eq!(parse_head(b"HTTP/2 200 OK\r\n\r\n"), Err(HeadError::Version), "версия чужая");
        assert_eq!(parse_head(b"HTTP/1.2 200 OK\r\n\r\n"), Err(HeadError::Version));
        assert_eq!(parse_head(b"ICY 200 OK\r\n\r\n"), Err(HeadError::StatusLine));
        assert_eq!(parse_head(b"HTTP/1.1\r\n\r\n"), Err(HeadError::StatusLine));
        for bad in ["HTTP/1.1 20 OK", "HTTP/1.1 2000 OK", "HTTP/1.1 abc OK", "HTTP/1.1 099 Low", "HTTP/1.1 +20 OK"] {
            assert_eq!(parse_head(format!("{bad}\r\n\r\n").as_bytes()), Err(HeadError::Status), "статус в {bad:?} недопустим");
        }
    }

    #[test]
    fn broken_headers() {
        for bad in ["Name value", ": empty-name", "Bad Name: v", "Name : v", " folded: continuation", "\tfolded: x", "Bad(Name): v"] {
            let wire = format!("HTTP/1.1 200 OK\r\nGood: 1\r\n{bad}\r\n\r\n");
            assert_eq!(parse_head(wire.as_bytes()), Err(HeadError::Header), "строка {bad:?} заголовком быть не может");
        }
    }

    #[test]
    fn endless_head_is_cut_off() {
        let mut wire = b"HTTP/1.1 200 OK\r\n".to_vec();
        while wire.len() < MAX_HEAD + 100 {
            wire.extend_from_slice(b"X-Pad: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
        }
        assert_eq!(parse_head(&wire), Err(HeadError::TooLarge), "сервер, который не кончает голову, не должен копить память клиента");
        wire.extend_from_slice(b"\r\n");
        assert_eq!(parse_head(&wire), Err(HeadError::TooLarge));
    }
}
