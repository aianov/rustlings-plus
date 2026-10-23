#[derive(Debug, PartialEq)]
pub enum QuotaError {
    Empty,
    NotNumber(String),
    TooBig { limit: u32, got: u32 },
}

const LIMIT: u32 = 1000;

pub fn parse_quota(text: &str) -> Result<u32, QuotaError> {
    if text.is_empty() {
        return Err(QuotaError::Empty);
    }
    let got: u32 = text.parse().map_err(|_| QuotaError::NotNumber(text.to_string()))?;
    if got > LIMIT {
        return Err(QuotaError::TooBig { limit: LIMIT, got });
    }
    Ok(got)
}

pub fn per_user(total: u32, users: u32) -> u32 {
    assert!(users > 0, "нет пользователей: делить квоту не на кого");
    total / users
}

fn main() {
    println!("{:?}", parse_quota("512"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_number() -> Result<(), QuotaError> {
        let quota = parse_quota("512")?;
        assert_eq!(quota, 512);
        Ok(())
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(parse_quota(""), Err(QuotaError::Empty)));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(parse_quota("12x"), Err(QuotaError::NotNumber(ref text)) if text == "12x"));
    }

    #[test]
    fn limit_is_inclusive() -> Result<(), QuotaError> {
        assert_eq!(parse_quota("1000")?, 1000);
        Ok(())
    }

    #[test]
    fn too_big_reports_the_limit() {
        assert!(matches!(parse_quota("1001"), Err(QuotaError::TooBig { limit: 1000, got: 1001 })));
    }

    #[test]
    #[should_panic(expected = "нет пользователей")]
    fn zero_users() {
        per_user(100, 0);
    }
}

#[cfg(test)]
mod grading {
    use super::*;
    const SRC: &str = include_str!("tst3.rs");

    fn tests_text() -> String {
        let code = SRC.split("#[cfg(test)]\nmod grading").next().unwrap_or("");
        let start = code.rfind("mod tests").expect("нет модуля tests");
        code[start..].to_string()
    }

    #[test]
    fn empty_is_its_own_error() {
        assert_eq!(parse_quota(""), Err(QuotaError::Empty), "пустая строка это Empty, а не NotNumber");
    }

    #[test]
    fn limit_and_garbage() {
        assert_eq!(parse_quota("1000"), Ok(1000), "лимит включительно");
        assert_eq!(parse_quota("1001"), Err(QuotaError::TooBig { limit: 1000, got: 1001 }));
        assert_eq!(parse_quota("-5"), Err(QuotaError::NotNumber("-5".to_string())));
    }

    #[test]
    fn zero_users_panics_with_a_message() {
        let result = std::panic::catch_unwind(|| per_user(100, 0));
        let payload = result.expect_err("деление на ноль пользователей должно паниковать");
        let text = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(text.contains("нет пользователей"), "паника должна объяснять причину словами, а не быть арифметической: {text:?}");
        assert_eq!(per_user(100, 4), 25);
    }

    #[test]
    fn should_panic_names_its_reason() {
        let text = tests_text();
        let mut count = 0;
        for part in text.split("#[should_panic").skip(1) {
            count += 1;
            assert!(part.trim_start().starts_with("(expected"), "у каждого should_panic должен быть `expected = \"...\"`");
        }
        assert!(count >= 1, "тест на панику должен остаться");
    }

    #[test]
    fn result_tests_use_question_mark() {
        let text = tests_text();
        assert!(text.contains("-> Result<(), QuotaError>"), "хотя бы один тест должен возвращать Result и пробрасывать ошибку через `?`");
        assert!(!text.contains(".unwrap()"), "в тестах не должно быть unwrap: либо `?` в тесте с Result, либо matches!");
    }

    #[test]
    fn error_variants_are_matched() {
        let text = tests_text();
        assert!(text.matches("matches!(").count() >= 3, "варианты ошибок проверяй через matches!, а не через is_err");
        assert!(!text.contains(".is_err()"), "is_err не отличает одну ошибку от другой");
    }
}
