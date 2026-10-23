use auth_service::{ADMIN_ENV, CORS_ENV, Config, HTTP_ADDR_ENV};
use service_runtime::Source;

#[test]
fn config_reads_three_addresses_and_the_cors_list() {
    let config = Config::from_source(&Source::from_pairs([
        (CORS_ENV, "https://social.example,  ,http://localhost:5173"),
        ("ACCESS_LOG", "yes"),
    ]))
    .expect("всё верно");
    assert_eq!(config.addr, "127.0.0.1:50051");
    assert_eq!(config.http_addr, "127.0.0.1:7881");
    assert_eq!(config.admin, "127.0.0.1:9881");
    assert_eq!(
        config.cors_origins,
        ["https://social.example", "http://localhost:5173"]
    );
    assert!(config.access_log && config.log.is_some());
}

#[test]
fn one_port_for_two_listeners_is_caught_before_bind() {
    let error = Config::from_source(&Source::from_pairs([
        ("AUTH_GRPC_ADDR", "127.0.0.1:7000"),
        (HTTP_ADDR_ENV, "127.0.0.1:7000"),
        (ADMIN_ENV, "127.0.0.1:7000"),
    ]))
    .expect_err("три слушателя на одном порту");
    assert_eq!(error.keys(), [HTTP_ADDR_ENV, ADMIN_ENV], "{error}");
    let text = error.to_string();
    assert!(
        text.contains("AUTH_GRPC_ADDR"),
        "сказано, с кем конфликт: {text}"
    );
}
