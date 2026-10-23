pub fn preview(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes < '…'.len_utf8() {
        return String::new();
    }
    let mut cut = max_bytes - '…'.len_utf8();
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &text[..cut])
}

pub fn initials(full_name: &str) -> String {
    full_name.split_whitespace().filter_map(|word| word.chars().next()).flat_map(char::to_uppercase).collect()
}

fn main() {
    println!("{}", preview("длинный-длинный пост", 10));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_untouched() {
        assert_eq!(preview("hello", 5), "hello");
        assert_eq!(preview("привет", 12), "привет");
        assert_eq!(preview("", 0), "");
    }

    #[test]
    fn ascii_is_cut_with_an_ellipsis() {
        assert_eq!(preview("hello world", 8), "hello…");
        assert!(preview("hello world", 8).len() <= 8);
    }

    #[test]
    fn cyrillic_never_panics_and_fits_the_limit() {
        let text = "Привет, это длинный пост на русском языке";
        for limit in 0..text.len() + 3 {
            let p = preview(text, limit);
            assert!(p.len() <= limit, "лимит {limit} байт, получили {} байт: {p}", p.len());
        }
        assert_eq!(preview(text, 12), "Прив…", "на лимите 12 помещается 9 байт, а значит четыре буквы (8 байт) и многоточие");
        assert_eq!(preview(text, 13), "Приве…", "на лимите 13 помещается 10 байт текста и многоточие");
    }

    #[test]
    fn emoji_are_not_split() {
        assert_eq!(preview("👍👍👍👍👍", 10), "👍…");
        assert_eq!(preview("👍👍👍👍👍", 11), "👍👍…");
        assert_eq!(preview("a👍b", 5), "a…", "эмодзи не помещается целиком, значит его нет совсем");
    }

    #[test]
    fn tiny_limits_give_an_empty_preview() {
        assert_eq!(preview("long text", 2), "");
        assert_eq!(preview("long text", 3), "…");
    }

    #[test]
    fn initials_take_a_char_not_a_byte() {
        assert_eq!(initials("Анна Мария Петрова"), "АМП");
        assert_eq!(initials("ada lovelace"), "AL");
        assert_eq!(initials("  élan   vital "), "ÉV");
        assert_eq!(initials(""), "");
    }
}
