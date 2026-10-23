pub fn greet(name: &str) -> String {
    if cfg!(feature = "shout") {
        format!("HELLO, {}!", name.to_uppercase())
    } else {
        format!("Hello, {name}!")
    }
}

pub fn shout(name: &str) -> String {
    format!("HELLO, {}!", name.to_uppercase())
}
