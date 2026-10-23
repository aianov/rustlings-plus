use crate::lexer::split_words;

pub fn extract(text: &str) -> Vec<String> {
    split_words(text)
        .into_iter()
        .filter_map(|w| w.strip_prefix('@').filter(|n| !n.is_empty()).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_mentions() {
        assert_eq!(extract("hi @ann and @bob!"), ["ann", "bob"]);
        assert!(extract("no one here @").is_empty());
    }
}
