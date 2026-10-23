pub fn red(text: &str) -> String {
    format!("\x1b[31m{text}\x1b[0m")
}
