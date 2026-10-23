use grpc_kit::{CLIENT_KEYS, CLIENT_VALUE_MAX, Stamp, client_value};
use tonic::Request;
use tonic::service::Interceptor;

fn sent(mut stamp: Stamp) -> Request<()> {
    stamp
        .call(Request::new(()))
        .expect("штамп с бюджетом без срока проходит")
}

fn text<'a>(request: &'a Request<()>, key: &str) -> Option<&'a str> {
    request.metadata().get(key).and_then(|v| v.to_str().ok())
}

#[test]
fn only_whitelisted_client_metadata_passes() {
    assert_eq!(CLIENT_KEYS, ["x-trace-id", "accept-language"]);
    let stamp = Stamp::new("gw-1")
        .user(7)
        .role("user")
        .pass("X-Trace-Id", "trace-7")
        .pass("accept-language", "ru-RU,ru;q=0.9")
        .pass("x-user-id", "1")
        .pass("x-user-role", "admin")
        .pass("x-request-id", "forged")
        .pass("x-internal-debug", "1");
    let request = sent(stamp);
    assert_eq!(
        (
            text(&request, "x-request-id"),
            text(&request, "x-user-id"),
            text(&request, "x-user-role")
        ),
        (Some("gw-1"), Some("7"), Some("user")),
        "внутренние ключи берутся только из проверенного контекста"
    );
    assert_eq!(text(&request, "x-trace-id"), Some("trace-7"));
    assert_eq!(text(&request, "accept-language"), Some("ru-RU,ru;q=0.9"));
    assert!(
        text(&request, "x-internal-debug").is_none(),
        "белый список, а не чёрный: незнакомый ключ не проходит"
    );
    let long = "x".repeat(CLIENT_VALUE_MAX + 1);
    for bad in ["a\nb", "a\rb", "", long.as_str(), "язык"] {
        assert!(!client_value(bad), "{bad:?} не годится в metadata");
        let request = sent(Stamp::new("gw-2").pass("x-trace-id", bad));
        assert!(text(&request, "x-trace-id").is_none(), "{bad:?}");
    }
    let twice = Stamp::new("gw-3")
        .pass("x-trace-id", "first")
        .pass("x-trace-id", "second");
    assert_eq!(
        twice.passed(),
        [("x-trace-id", String::from("second"))],
        "ключ один, побеждает последнее значение"
    );
}

#[test]
fn services_carry_client_metadata_further() {
    let incoming = sent(
        Stamp::new("gw-4")
            .user(3)
            .pass("x-trace-id", "trace-9")
            .pass("accept-language", "en"),
    );
    let onward = sent(Stamp::follow(&incoming));
    assert_eq!(
        (
            text(&onward, "x-request-id"),
            text(&onward, "x-trace-id"),
            text(&onward, "accept-language")
        ),
        (Some("gw-4"), Some("trace-9"), Some("en")),
        "Stamp::follow несёт номер запроса и клиентский след дальше по цепочке"
    );
}
