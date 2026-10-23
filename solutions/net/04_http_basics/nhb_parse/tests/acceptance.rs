use http_parse::*;

fn limits() -> Limits {
    Limits::default()
}

fn complete(buf: &[u8]) -> (Request, usize) {
    match parse_request(buf, &limits()) {
        Ok(Parsed::Complete { request, consumed }) => (request, consumed),
        other => panic!("ожидался готовый запрос, получено {other:?}"),
    }
}

fn st(code: u16) -> StatusCode {
    StatusCode::new(code).unwrap_or_else(|| panic!("код {code} должен быть допустим"))
}

const GET: &[u8] = b"GET /books/42?fields=title HTTP/1.1\r\nHost: shop.test\r\nAccept: */*\r\n\r\n";
const POST: &[u8] = b"POST /books HTTP/1.1\r\nHost: shop.test\r\nContent-Type: text/plain\r\nContent-Length: 11\r\n\r\nline1\r\n\r\nok";
const CHUNKED: &[u8] = b"POST /upload HTTP/1.1\r\nHost: shop.test\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\nX-T: 1\r\n\r\n";

#[test]
fn simple_get() {
    let (req, used) = complete(GET);
    assert_eq!(req.method, Method::Get);
    assert_eq!(req.version, Version::Http11);
    assert_eq!(req.target, "/books/42?fields=title");
    assert_eq!(req.headers.get("host"), Some("shop.test"));
    assert_eq!(req.headers.get("ACCEPT"), Some("*/*"), "имена заголовков без учёта регистра");
    assert_eq!(req.headers.len(), 2);
    assert!(req.body.is_empty());
    assert_eq!(used, GET.len());
    assert!(req.keep_alive(), "HTTP/1.1 держит соединение по умолчанию");
}

#[test]
fn body_and_every_split_point() {
    for full in [GET, POST, CHUNKED] {
        for cut in 0..full.len() {
            assert_eq!(parse_request(&full[..cut], &limits()), Ok(Parsed::Partial), "на {cut} байтах из {} запрос ещё не готов", full.len());
        }
    }
    let (req, used) = complete(POST);
    assert_eq!(req.body, b"line1\r\n\r\nok", "пустая строка внутри тела не конец заголовков");
    assert_eq!(used, POST.len());
}

#[test]
fn chunked_request_body() {
    let (req, used) = complete(CHUNKED);
    assert_eq!(req.body, b"Wikipedia");
    assert_eq!(used, CHUNKED.len(), "трейлеры входят в consumed");
}

#[test]
fn pipelining_keeps_boundaries() {
    let mut data = GET.to_vec();
    data.extend_from_slice(POST);
    data.extend_from_slice(CHUNKED);
    data.extend_from_slice(b"GET /last HTTP/1.1\r\nHo");
    let mut pos = 0;
    let mut targets = Vec::new();
    for _ in 0..3 {
        let (req, used) = complete(&data[pos..]);
        targets.push(req.target);
        pos += used;
    }
    assert_eq!(targets, ["/books/42?fields=title", "/books", "/upload"]);
    assert_eq!(parse_request(&data[pos..], &limits()), Ok(Parsed::Partial), "четвёртый запрос обрезан");
}

#[test]
fn path_and_query_are_decoded_separately() {
    let (req, _) = complete(b"GET /a%20b/c?x=1%262&y=a+b&flag&e=%D0%AF HTTP/1.1\r\nHost: h\r\n\r\n");
    assert_eq!(req.path(), Ok("/a b/c".to_string()));
    let q: Vec<(String, String)> = [("x", "1&2"), ("y", "a b"), ("flag", ""), ("e", "Я")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert_eq!(req.query(), Ok(q));
    let (req, _) = complete(b"GET /p+q HTTP/1.1\r\nHost: h\r\n\r\n");
    assert_eq!(req.path(), Ok("/p+q".to_string()), "в пути плюс остаётся плюсом");
    assert_eq!(req.query(), Ok(vec![]), "запроса нет совсем");
    let (req, _) = complete(b"GET /?q=%zz HTTP/1.1\r\nHost: h\r\n\r\n");
    assert_eq!(req.query(), Err(ParseError::BadEscape));
}

#[test]
fn malformed_heads() {
    let cases: [(&[u8], ParseError); 8] = [
        (b"BREW / HTTP/1.1\r\nHost: h\r\n\r\n", ParseError::BadMethod),
        (b"GET / HTTP/2.0\r\nHost: h\r\n\r\n", ParseError::BadVersion),
        (b"GET  / HTTP/1.1\r\nHost: h\r\n\r\n", ParseError::BadRequestLine),
        (b"GET relative HTTP/1.1\r\nHost: h\r\n\r\n", ParseError::BadTarget),
        (b"GET / HTTP/1.1\r\nHost : h\r\n\r\n", ParseError::BadHeader),
        (b"GET / HTTP/1.1\r\nHost: h\r\nBad\r\n\r\n", ParseError::BadHeader),
        (b"GET / HTTP/1.1\r\nHost: h\r\nX: a\x00b\r\n\r\n", ParseError::BadHeader),
        (b"GET / HTTP/1.1\r\nHost: \xff\xfe\r\n\r\n", ParseError::BadHeader),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_request(input, &limits()), Err(expected), "для {:?}", String::from_utf8_lossy(input));
    }
}

#[test]
fn host_rules() {
    assert_eq!(parse_request(b"GET / HTTP/1.1\r\n\r\n", &limits()), Err(ParseError::MissingHost));
    assert_eq!(parse_request(b"GET / HTTP/1.1\r\nHost:\r\n\r\n", &limits()), Err(ParseError::MissingHost));
    assert_eq!(parse_request(b"GET / HTTP/1.1\r\nHost: a\r\nhost: b\r\n\r\n", &limits()), Err(ParseError::DuplicateHost));
    let (req, _) = complete(b"GET / HTTP/1.0\r\n\r\n");
    assert_eq!(req.version, Version::Http10, "в HTTP/1.0 Host не обязателен");
    assert!(!req.keep_alive(), "HTTP/1.0 по умолчанию закрывает");
}

#[test]
fn keep_alive_follows_connection_header() {
    let (r, _) = complete(b"GET / HTTP/1.1\r\nHost: h\r\nConnection: keep-alive, Close\r\n\r\n");
    assert!(!r.keep_alive(), "close в списке побеждает");
    let (r, _) = complete(b"GET / HTTP/1.0\r\nConnection: Keep-Alive\r\n\r\n");
    assert!(r.keep_alive());
}

#[test]
fn smuggling_is_refused() {
    let both = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
    assert_eq!(parse_request(both, &limits()), Err(ParseError::Smuggling), "Content-Length вместе с Transfer-Encoding");
    let two = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\nContent-Length: 5\r\n\r\nabcde";
    assert_eq!(parse_request(two, &limits()), Err(ParseError::Smuggling), "два разных Content-Length");
    let plus = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: +4\r\n\r\nabcd";
    assert_eq!(parse_request(plus, &limits()), Err(ParseError::BadContentLength));
    let same = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\ncontent-length: 2\r\n\r\nhi";
    assert_eq!(complete(same).0.body, b"hi", "одинаковые повторы безвредны");
}

#[test]
fn limits_work_before_data_arrives() {
    let small = Limits { max_head_bytes: 64, max_headers: 3, max_body: 8 };
    let long = format!("GET / HTTP/1.1\r\nHost: h\r\nX-Long: {}\r\n\r\n", "a".repeat(100));
    assert_eq!(parse_request(long.as_bytes(), &small), Err(ParseError::HeadTooLarge));
    assert_eq!(parse_request(&long.as_bytes()[..80], &small), Err(ParseError::HeadTooLarge), "голова без конца уже длиннее лимита: ждать нечего");
    let many = b"GET / HTTP/1.1\r\nHost: h\r\nA: 1\r\nB: 2\r\nC: 3\r\n\r\n";
    assert_eq!(parse_request(many, &small), Err(ParseError::TooManyHeaders));
    let big_head_only = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 9\r\n\r\n";
    assert_eq!(parse_request(big_head_only, &small), Err(ParseError::BodyTooLarge), "Content-Length больше лимита отвергается сразу, тела ещё нет");
    let big_chunk = b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFF\r\n";
    assert_eq!(parse_request(big_chunk, &small), Err(ParseError::BodyTooLarge));
    let ok = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 8\r\n\r\n12345678";
    assert_eq!(complete_with(ok, &small).0.body, b"12345678", "ровно в лимит можно");
}

fn complete_with(buf: &[u8], limits: &Limits) -> (Request, usize) {
    match parse_request(buf, limits) {
        Ok(Parsed::Complete { request, consumed }) => (request, consumed),
        other => panic!("ожидался готовый запрос, получено {other:?}"),
    }
}

#[test]
fn headers_container() {
    let mut h = Headers::new();
    h.append("Set-Cookie", "a=1");
    h.append("Accept", "*/*");
    h.append("set-cookie", "b=2");
    assert_eq!(h.get_all("SET-COOKIE"), vec!["a=1", "b=2"]);
    assert_eq!(h.get("set-cookie"), Some("a=1"));
    let names: Vec<&str> = h.iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["Set-Cookie", "Accept", "set-cookie"], "порядок и исходный регистр сохраняются");
    assert_eq!(h.remove("Set-Cookie"), 2);
    assert_eq!(h.len(), 1);
    assert!(!h.is_empty());
}

#[test]
fn methods_roundtrip() {
    for (text, m) in [("GET", Method::Get), ("HEAD", Method::Head), ("POST", Method::Post), ("PUT", Method::Put), ("DELETE", Method::Delete), ("PATCH", Method::Patch), ("OPTIONS", Method::Options)] {
        assert_eq!(text.parse::<Method>(), Ok(m));
        assert_eq!(m.as_str(), text);
    }
    assert_eq!("get".parse::<Method>(), Err(ParseError::BadMethod));
}

#[test]
fn status_codes() {
    assert_eq!(StatusCode::new(99), None);
    assert_eq!(StatusCode::new(600), None);
    assert_eq!(st(404).reason(), "Not Found");
    assert_eq!(st(299).reason(), "");
    assert_eq!(st(500).code(), 500);
    assert!(st(200).allows_body());
    assert!(!st(204).allows_body() && !st(304).allows_body() && !st(100).allows_body());
}

#[test]
fn response_serialization() {
    let mut r = Response::new(st(200));
    r.header("Content-Type", "text/plain").unwrap();
    r.body = b"hello".to_vec();
    assert_eq!(String::from_utf8_lossy(&r.to_bytes(false)), "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello");
    assert_eq!(String::from_utf8_lossy(&r.to_bytes(true)), "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\n", "на HEAD тело не пишется, длина остаётся");
    let mut no = Response::new(st(204));
    no.body = b"nope".to_vec();
    assert_eq!(no.to_bytes(false), b"HTTP/1.1 204 No Content\r\n\r\n", "у 204 нет ни тела, ни Content-Length");
    let mut own = Response::new(st(200));
    own.header("content-length", "3").unwrap();
    own.body = b"abc".to_vec();
    let text = String::from_utf8_lossy(&own.to_bytes(false)).to_ascii_lowercase();
    assert_eq!(text.matches("content-length").count(), 1);
    assert!(Response::new(st(299)).to_bytes(false).starts_with(b"HTTP/1.1 299 \r\n"));
}

#[test]
fn response_splitting_is_refused() {
    let mut r = Response::new(st(302));
    assert_eq!(r.header("Location", "/x\r\nSet-Cookie: admin=1"), Err(ParseError::BadHeader));
    assert_eq!(r.header("Location", "/x\nSet-Cookie: admin=1"), Err(ParseError::BadHeader));
    assert_eq!(r.header("Bad Name", "v"), Err(ParseError::BadHeader));
    assert!(r.header("Location", "/ok\tfine").is_ok());
    assert_eq!(r.headers.len(), 1);
}

#[test]
fn parser_never_panics_on_garbage() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let alphabet: &[u8] = b"GETPOSHPTA/1.0: \r\n%+?&=;0123456789abcdefABCDEFxyz\xff\x00\xc3";
    let templates: [&[u8]; 3] = [GET, POST, CHUNKED];
    for round in 0..3000 {
        let mut buf: Vec<u8> = if round % 2 == 0 {
            (0..(next() % 120) as usize).map(|_| alphabet[(next() % alphabet.len() as u64) as usize]).collect()
        } else {
            templates[(next() % 3) as usize].to_vec()
        };
        if round % 2 == 1 && !buf.is_empty() {
            for _ in 0..(next() % 4) {
                let i = (next() % buf.len() as u64) as usize;
                buf[i] = alphabet[(next() % alphabet.len() as u64) as usize];
            }
            buf.truncate((next() % (buf.len() as u64 + 1)) as usize);
        }
        let _ = parse_request(&buf, &limits());
    }
}

#[test]
fn transfer_encoding_must_be_exactly_chunked() {
    for te in ["gzip, chunked", "gzip", "chunked, chunked", "identity"] {
        let raw = format!("POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: {te}\r\n\r\n0\r\n\r\n");
        assert_eq!(parse_request(raw.as_bytes(), &limits()), Err(ParseError::BadHeader), "Transfer-Encoding: {te} не понимаем и не угадываем");
    }
}
