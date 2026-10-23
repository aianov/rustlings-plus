pub fn first_byte(text: &str) -> Option<u8> {
    text.as_bytes().first().copied()
}

pub fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse().ok()
}

pub fn slug(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
