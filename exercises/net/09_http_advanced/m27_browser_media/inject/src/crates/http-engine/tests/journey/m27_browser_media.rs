use http_engine::{
    BodyReader, CorsPolicy, Framing, Handler, Method, RangeSpec, Request, Response, SameSite,
    SetCookie, apply, cookie, cors, etag, handler, parse_cookies, parse_range, request_cookie,
    serve_bytes, token_from_cookie,
};
use tokio::io::AsyncReadExt;

fn request(method: Method, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Request {
    let mut request = Request::new(method, target);
    for (name, value) in headers {
        request.headers.append(name, value);
    }
    request.body = body.to_vec();
    request
}

fn header(r: &Response, name: &str) -> Option<String> {
    r.headers.get(name).map(str::to_string)
}

fn vary(r: &Response) -> Vec<String> {
    r.headers
        .get_all("vary")
        .iter()
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .collect()
}

fn policy(origins: &[&str], credentials: bool) -> CorsPolicy {
    CorsPolicy {
        origins: origins.iter().map(|s| s.to_string()).collect(),
        methods: vec!["GET".into(), "PUT".into(), "DELETE".into()],
        headers: vec!["Content-Type".into(), "Authorization".into()],
        expose_headers: vec!["ETag".into()],
        credentials,
        max_age: 600,
    }
}

const APP: &str = "https://app.example";

fn inner(status: u16) -> Handler {
    handler(move |_| async move {
        let mut r = Response::new(status);
        r.headers.append("Vary", "Accept-Encoding");
        r
    })
}

#[test]
fn cookies_are_parsed_and_written() {
    let pairs = parse_cookies("theme=dark; token=eyJ=; bad name=1; sid=\"abc\"; sid=second");
    assert_eq!(cookie(&pairs, "theme"), Some("dark"));
    assert_eq!(cookie(&pairs, "token"), Some("eyJ="));
    assert_eq!(
        cookie(&pairs, "sid"),
        Some("abc"),
        "первое значение, кавычки сняты"
    );
    assert_eq!(cookie(&pairs, "bad name"), None);
    let mut c = SetCookie::new("refresh", "t0k3n");
    c.max_age = Some(2_592_000);
    c.path = Some("/auth".into());
    c.http_only = true;
    c.secure = true;
    c.same_site = Some(SameSite::Lax);
    assert_eq!(
        c.to_header().expect("cookie собирается"),
        "refresh=t0k3n; Max-Age=2592000; Path=/auth; HttpOnly; Secure; SameSite=Lax"
    );
    assert_eq!(
        SetCookie::delete("refresh", "/auth")
            .to_header()
            .expect("cookie собирается"),
        "refresh=; Max-Age=0; Path=/auth"
    );
    assert!(SetCookie::new("a", "x y").to_header().is_err());
    let mut none = SetCookie::new("a", "1");
    none.same_site = Some(SameSite::None);
    assert!(none.to_header().is_err(), "SameSite=None без Secure");
}

#[tokio::test]
async fn cors_layer_answers_preflight_and_decorates_everything_else() {
    let app = apply(inner(500), &[cors(policy(&[APP], true))]);
    let pre = request(
        Method::Options,
        "/x",
        &[
            ("Origin", APP),
            ("Access-Control-Request-Method", "PUT"),
            (
                "Access-Control-Request-Headers",
                "content-type, authorization",
            ),
        ],
        b"",
    );
    let r = app(pre).await;
    assert_eq!(
        r.status.code(),
        204,
        "предзапрос обрабатывает слой, обработчик с его 500 не вызывается"
    );
    assert_eq!(
        header(&r, "access-control-allow-origin").as_deref(),
        Some(APP)
    );
    assert_eq!(
        header(&r, "access-control-allow-credentials").as_deref(),
        Some("true")
    );
    assert_eq!(header(&r, "access-control-max-age").as_deref(), Some("600"));
    for needed in [
        "origin",
        "access-control-request-method",
        "access-control-request-headers",
    ] {
        assert!(vary(&r).contains(&needed.to_string()), "Vary без {needed}");
    }
    let bad = request(
        Method::Options,
        "/x",
        &[("Origin", APP), ("Access-Control-Request-Method", "PATCH")],
        b"",
    );
    assert_eq!(app(bad).await.status.code(), 403);
    let evil = request(
        Method::Options,
        "/x",
        &[
            ("Origin", "https://evil.example"),
            ("Access-Control-Request-Method", "GET"),
        ],
        b"",
    );
    let r = app(evil).await;
    assert_eq!(r.status.code(), 403);
    assert!(header(&r, "access-control-allow-origin").is_none());

    let r = app(request(Method::Get, "/x", &[("Origin", APP)], b"")).await;
    assert_eq!(r.status.code(), 500);
    assert_eq!(
        header(&r, "access-control-allow-origin").as_deref(),
        Some(APP),
        "ответ об ошибке тоже должен быть читаем фронтом"
    );
    assert_eq!(
        header(&r, "access-control-expose-headers").as_deref(),
        Some("ETag")
    );
    assert!(
        vary(&r).contains(&"origin".to_string())
            && vary(&r).contains(&"accept-encoding".to_string()),
        "{:?}",
        vary(&r)
    );
    let r = app(request(
        Method::Get,
        "/x",
        &[("Origin", "https://evil.example")],
        b"",
    ))
    .await;
    assert!(header(&r, "access-control-allow-origin").is_none());
    assert!(
        vary(&r).contains(&"origin".to_string()),
        "Vary нужен и чужому"
    );
    let r = app(request(Method::Options, "/x", &[], b"")).await;
    assert_eq!(
        r.status.code(),
        500,
        "обычный OPTIONS проходит к обработчику"
    );
}

#[tokio::test]
async fn cors_wildcard_rules() {
    let open = apply(inner(200), &[cors(policy(&["*"], false))]);
    let r = open(request(
        Method::Get,
        "/x",
        &[("Origin", "https://any.example")],
        b"",
    ))
    .await;
    assert_eq!(
        header(&r, "access-control-allow-origin").as_deref(),
        Some("*")
    );
    assert!(!vary(&r).contains(&"origin".to_string()));
    let creds = apply(inner(200), &[cors(policy(&["*"], true))]);
    let r = creds(request(
        Method::Get,
        "/x",
        &[("Origin", "https://any.example")],
        b"",
    ))
    .await;
    assert_eq!(
        header(&r, "access-control-allow-origin").as_deref(),
        Some("https://any.example"),
        "'*' с credentials недопустима"
    );
    assert_eq!(
        header(&r, "access-control-allow-credentials").as_deref(),
        Some("true")
    );
    assert!(vary(&r).contains(&"origin".to_string()));
}

#[tokio::test]
async fn bytes_are_served_conditionally_and_partially() {
    let data: Vec<u8> = (0..=255u8).collect();
    let tag = etag(&data);
    let r = serve_bytes(
        &request(Method::Get, "/f", &[], b""),
        "application/octet-stream",
        &data,
    );
    assert_eq!((r.status.code(), r.body.len()), (200, 256));
    assert_eq!(header(&r, "etag").as_deref(), Some(tag.as_str()));
    assert_eq!(header(&r, "accept-ranges").as_deref(), Some("bytes"));
    let r = serve_bytes(
        &request(
            Method::Get,
            "/f",
            &[("If-None-Match", &format!("\"zzz\", W/{tag}"))],
            b"",
        ),
        "application/octet-stream",
        &data,
    );
    assert_eq!(r.status.code(), 304);
    assert!(r.body.is_empty());
    assert_eq!(header(&r, "etag").as_deref(), Some(tag.as_str()));
    let r = serve_bytes(
        &request(Method::Get, "/f", &[("Range", "bytes=250-")], b""),
        "application/octet-stream",
        &data,
    );
    assert_eq!(
        (r.status.code(), r.body.clone()),
        (206, data[250..].to_vec())
    );
    assert_eq!(
        header(&r, "content-range").as_deref(),
        Some("bytes 250-255/256")
    );
    let r = serve_bytes(
        &request(Method::Get, "/f", &[("Range", "bytes=-4")], b""),
        "application/octet-stream",
        &data,
    );
    assert_eq!(r.body, &data[252..]);
    let r = serve_bytes(
        &request(Method::Get, "/f", &[("Range", "bytes=999-")], b""),
        "application/octet-stream",
        &data,
    );
    assert_eq!(r.status.code(), 416);
    assert_eq!(header(&r, "content-range").as_deref(), Some("bytes */256"));
    let r = serve_bytes(
        &request(Method::Get, "/f", &[("Range", "bytes=5-2")], b""),
        "application/octet-stream",
        &data,
    );
    assert_eq!(r.status.code(), 200, "испорченный диапазон игнорируется");
    let r = serve_bytes(
        &request(
            Method::Get,
            "/f",
            &[("If-None-Match", &tag), ("Range", "bytes=0-9")],
            b"",
        ),
        "application/octet-stream",
        &data,
    );
    assert_eq!(r.status.code(), 304, "условие проверяется раньше диапазона");
    assert_eq!(
        parse_range(Some("bytes=0-0"), 1),
        RangeSpec::Partial { start: 0, end: 0 }
    );
    assert_eq!(parse_range(Some("bytes=0-"), 0), RangeSpec::Unsatisfiable);
}

#[tokio::test]
async fn body_reader_streams_and_stops_floods() {
    let wire: &[u8] = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\nNEXT";
    let mut body =
        BodyReader::new(wire, Vec::new(), Framing::Chunked, 100).expect("читатель создаётся");
    let mut all = Vec::new();
    while let Some(piece) = body.next_chunk().await.expect("кусок читается") {
        all.extend_from_slice(&piece);
    }
    assert_eq!(all, b"hello world");
    assert!(body.is_done());
    let (mut rest, buf) = body.into_parts();
    let mut tail = Vec::new();
    rest.read_to_end(&mut tail).await.expect("хвост читается");
    assert_eq!([buf, tail].concat(), b"NEXT");

    assert!(
        BodyReader::new(&b""[..], Vec::new(), Framing::Length(5000), 1000).is_err(),
        "заявлено больше лимита: отказ без чтения"
    );
    let big = vec![b'x'; 20_000];
    let mut body = BodyReader::new(&big[..], Vec::new(), Framing::Length(20_000), 1 << 20)
        .expect("читатель создаётся");
    let mut biggest = 0;
    let mut total = 0;
    while let Some(piece) = body.next_chunk().await.expect("кусок читается") {
        biggest = biggest.max(piece.len());
        total += piece.len();
    }
    assert_eq!(total, 20_000);
    assert!(biggest <= 4096, "кусок {biggest}");
    let mut flood = BodyReader::new(&b"ffff\r\n"[..], Vec::new(), Framing::Chunked, 1000)
        .expect("читатель создаётся");
    assert!(
        flood.next_chunk().await.is_err(),
        "размер куска больше лимита"
    );
    let truncated = BodyReader::new(&b"abc"[..], Vec::new(), Framing::Length(10), 100);
    let mut truncated = truncated.expect("читатель создаётся");
    let mut failed = false;
    for _ in 0..4 {
        if truncated.next_chunk().await.is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed, "обрыв посреди тела это ошибка");
    let mut skipped = BodyReader::new(&b"0123456789"[..], Vec::new(), Framing::Length(10), 100)
        .expect("читатель создаётся");
    assert!(
        skipped.drain(100).await.expect("тело вычитывается"),
        "небольшое тело вычитывается"
    );
    let mut too_long = BodyReader::new(&big[..], Vec::new(), Framing::Length(20_000), 1 << 20)
        .expect("читатель создаётся");
    assert!(
        !too_long.drain(5000).await.expect("тело вычитывается"),
        "больше предела вычитывать не будем"
    );
}

#[tokio::test]
async fn session_cookie_becomes_a_bearer_token() {
    let echo = handler(|request: Request| async move {
        Response::text(200, request.headers.get("authorization").unwrap_or("нет"))
    });
    let app = apply(echo, &[token_from_cookie("session")]);
    let from_cookie = app(request(
        Method::Get,
        "/",
        &[("Cookie", "theme=dark; session=t0k3n")],
        b"",
    ))
    .await;
    assert_eq!(
        from_cookie.body, b"Bearer t0k3n",
        "токен из cookie становится заголовком Authorization"
    );
    let both = app(request(
        Method::Get,
        "/",
        &[("Authorization", "Bearer own"), ("Cookie", "session=t0k3n")],
        b"",
    ))
    .await;
    assert_eq!(
        both.body, b"Bearer own",
        "явный Authorization важнее cookie"
    );
    let none = app(request(Method::Get, "/", &[("Cookie", "theme=dark")], b"")).await;
    assert_eq!(none.body, "нет".as_bytes(), "без cookie ничего не меняется");
    let raw = request(
        Method::Get,
        "/",
        &[("Cookie", "a=1"), ("Cookie", "session=x")],
        b"",
    );
    assert_eq!(
        request_cookie(&raw, "session").as_deref(),
        Some("x"),
        "cookie ищется во всех заголовках Cookie"
    );
}
