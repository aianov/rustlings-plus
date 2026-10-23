#[test]
fn greet_is_plain_whatever_features_are_on() {
    assert_eq!(greeter::greet("Ann"), "Hello, Ann!", "greet не должна менять поведение от features");
}

#[test]
fn shout_is_always_available() {
    assert_eq!(greeter::shout("Ann"), "HELLO, ANN!");
}
