#[test]
fn quiet_app_greets_politely() {
    assert_eq!(quiet_app::welcome("Ann"), "Hello, Ann!", "quiet-app должен здороваться обычным голосом");
}
