#[test]
fn loud_app_announces_loudly() {
    assert_eq!(loud_app::announce("Ann"), "HELLO, ANN!", "loud-app должен кричать");
}
