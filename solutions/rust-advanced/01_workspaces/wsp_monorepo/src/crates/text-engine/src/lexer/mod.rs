pub fn split_words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '@' || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}
