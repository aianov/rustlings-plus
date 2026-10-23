use http_engine::{Headers, Response, header_safe, token_name};

#[test]
fn header_values_cannot_split_a_response() {
    let mut response = Response::new(200);
    response
        .headers
        .set("X-Name", "alice\r\nSet-Cookie: session=stolen");
    assert_eq!(
        response.headers.get("x-name"),
        Some("alice  Set-Cookie: session=stolen"),
        "перевод строки в значении становится пробелом"
    );
    assert_eq!(
        response.headers.get("set-cookie"),
        None,
        "нового заголовка не появилось"
    );
    let mut headers = Headers::new();
    headers.append("Bad\r\nName", "x");
    headers.append("", "x");
    assert!(headers.is_empty(), "имя заголовка только из символов token");
    assert_eq!(header_safe("a\0b\nc"), "a b c");
    assert!(token_name("X-Request-Id") && !token_name("X Request"));
}
