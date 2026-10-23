use std::ops::Range;

pub fn find_mention(text: &str, handle: &str) -> Option<Range<usize>> {
    let needle = format!("@{handle}");
    let start = text.as_bytes().windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))?;
    Some(start..start + needle.len())
}

pub fn canonical_handle(handle: &str) -> String {
    handle.to_ascii_lowercase()
}

fn main() {
    println!("{:?}", find_mention("hi @Ira", "ira"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_mention_is_found() {
        assert_eq!(find_mention("hello @Ira, how are you", "ira"), Some(6..10));
        assert_eq!(find_mention("@IRA", "ira"), Some(0..4));
        assert_eq!(find_mention("nobody here", "ira"), None);
        assert_eq!(find_mention("ira without at", "ira"), None);
    }

    #[test]
    fn offsets_belong_to_the_original_text() {
        let text = "İstanbul и Москва, привет @Ira!";
        let found = find_mention(text, "ira").expect("упоминание есть");
        assert_eq!(&text[found.clone()], "@Ira", "смещения должны указывать в исходный текст, получили {found:?}");
    }

    #[test]
    fn the_first_mention_wins_and_short_texts_are_safe() {
        assert_eq!(find_mention("@a @A", "a"), Some(0..2));
        assert_eq!(find_mention("@", "ira"), None);
        assert_eq!(find_mention("", "ira"), None);
    }

    #[test]
    fn canonical_handles_fold_ascii_only() {
        assert_eq!(canonical_handle("Ira_K"), "ira_k");
        assert_eq!(canonical_handle("ÄRGER_Bob"), "Ärger_bob", "не-ASCII буквы остаются как были");
        assert_eq!(canonical_handle("Привет"), "Привет");
    }
}
