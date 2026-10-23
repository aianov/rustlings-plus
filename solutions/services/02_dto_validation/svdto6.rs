#[derive(Debug, PartialEq)]
pub enum NameError {
    Empty,
    TooLong { max: usize },
}

pub fn validate_display_name(raw: &str) -> Result<String, NameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > 10 {
        return Err(NameError::TooLong { max: 10 });
    }
    Ok(name.to_string())
}

pub fn preview(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let head: String = name.chars().take(max_chars).collect();
    format!("{head}…")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyrillic_names_are_measured_in_chars() {
        assert_eq!(validate_display_name("Анастасия"), Ok("Анастасия".to_string()), "девять букв проходят");
        assert_eq!(validate_display_name("Александрина"), Err(NameError::TooLong { max: 10 }), "двенадцать букв много");
        assert_eq!(validate_display_name("Анастасия1"), Ok("Анастасия1".to_string()), "ровно десять символов проходят");
    }

    #[test]
    fn blank_name_is_empty() {
        assert_eq!(validate_display_name("    "), Err(NameError::Empty), "из пробелов это пустое имя");
        assert_eq!(validate_display_name(""), Err(NameError::Empty), "и пустая строка тоже");
    }

    #[test]
    fn surrounding_spaces_are_trimmed_before_checks_and_saving() {
        assert_eq!(validate_display_name("  Боб  "), Ok("Боб".to_string()), "результат без пробелов по краям");
        assert_eq!(
            validate_display_name("  Анастасия1  "),
            Ok("Анастасия1".to_string()),
            "десять символов плюс пробелы по краям всё ещё десять"
        );
    }

    #[test]
    fn preview_never_panics_on_multibyte_text() {
        assert_eq!(preview("Анастасия", 4), "Анас…", "обрезаем по символам");
        assert_eq!(preview("日本語のテキスト", 3), "日本語…", "иероглифы");
        assert_eq!(preview("👍👍👍👍", 2), "👍👍…", "смайлики");
    }

    #[test]
    fn short_names_are_left_alone() {
        assert_eq!(preview("Боб", 5), "Боб", "короткое имя не трогаем");
        assert_eq!(preview("Анастасия", 9), "Анастасия", "ровно по длине тоже");
    }
}
