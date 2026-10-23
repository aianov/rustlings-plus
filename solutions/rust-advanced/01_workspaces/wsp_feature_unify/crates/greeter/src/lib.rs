pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

pub fn shout(name: &str) -> String {
    format!("HELLO, {}!", name.to_uppercase())
}
