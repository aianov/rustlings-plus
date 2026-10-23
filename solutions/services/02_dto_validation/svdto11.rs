pub fn normalize_username(raw: &str) -> String {
    raw.trim().to_lowercase()
}

pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

pub fn normalize_display_name(raw: &str) -> String {
    let visible: String = raw.chars().filter(|c| !is_invisible(*c)).collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize_tag(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('#');
    trimmed.split_whitespace().collect::<Vec<_>>().join("-").to_lowercase()
}

fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_and_emails_are_trimmed_and_lowercased() {
        assert_eq!(normalize_username("  Alice_01 "), "alice_01", "ник: trim и нижний регистр");
        assert_eq!(normalize_email(" Alice@Example.COM\n"), "alice@example.com", "почта: trim и нижний регистр");
    }

    #[test]
    fn lowercase_is_unicode_aware() {
        assert_eq!(normalize_username("АЛИСА"), "алиса", "кириллица тоже приводится к нижнему регистру");
    }

    #[test]
    fn display_name_collapses_whitespace() {
        assert_eq!(normalize_display_name("  Анна   Мария \t Иванова "), "Анна Мария Иванова", "повторные пробелы схлопываются в один");
        assert_eq!(normalize_display_name("Bob"), "Bob", "регистр в отображаемом имени сохраняется");
    }

    #[test]
    fn display_name_loses_invisible_characters() {
        assert_eq!(normalize_display_name("Bo\u{200B}b"), "Bob", "нулевой пробел вырезается");
        assert_eq!(normalize_display_name("\u{FEFF}Bob\u{200D}"), "Bob", "BOM и нулевой соединитель тоже");
        assert_eq!(normalize_display_name("\u{200B}   \u{200B}"), "", "если остались одни невидимые, имя пустое");
    }

    #[test]
    fn tags_lose_the_hash_and_join_words() {
        assert_eq!(normalize_tag("  #Rust Lang "), "rust-lang", "решётка убирается, слова склеиваются дефисом");
        assert_eq!(normalize_tag("TOKIO"), "tokio", "нижний регистр");
        assert_eq!(normalize_tag("##x"), "x", "несколько решёток подряд");
    }

    #[test]
    fn normalization_is_idempotent() {
        for raw in ["  Alice ", "#Rust Lang", "A\u{200B}  B"] {
            let once = normalize_display_name(raw);
            assert_eq!(normalize_display_name(&once), once, "повторная нормализация ничего не меняет: {raw:?}");
            let tag = normalize_tag(raw);
            assert_eq!(normalize_tag(&tag), tag, "и для тегов: {raw:?}");
        }
    }
}
