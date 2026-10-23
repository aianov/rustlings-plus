pub const LIKE_ESCAPE: char = '\\';

pub fn escape_like(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for c in raw.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push(LIKE_ESCAPE);
        }
        escaped.push(c);
    }
    escaped
}

pub fn contains_pattern(raw: &str) -> String {
    format!("%{}%", escape_like(raw))
}
