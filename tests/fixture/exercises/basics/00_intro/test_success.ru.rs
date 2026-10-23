// Здесь уже всё работает.
fn main() {
    // Выведи что-нибудь.
    println!("Output from `main` function");
}

#[cfg(test)]
mod tests {
    #[test]
    fn pass() {}
}
