pub fn first_byte(text: &str) -> u8 {
    unsafe { *text.as_bytes().get_unchecked(0) }
}

pub fn parse_port(text: &str) -> u16 {
    text.trim().parse().unwrap()
}

pub fn slug(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
