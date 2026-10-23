#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub code: &'static str,
}

pub struct ListQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub tags: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct Page {
    pub limit: u32,
    pub offset: u32,
    pub tags: Vec<String>,
}

fn err(field: &str, code: &'static str) -> FieldError {
    FieldError { field: field.to_string(), code }
}

pub fn validate_list_query(query: &ListQuery) -> Result<Page, Vec<FieldError>> {
    let mut errors = Vec::new();
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        errors.push(err("limit", "out_of_range"));
    }
    let offset = query.offset.unwrap_or(0);
    if offset > 10_000 {
        errors.push(err("offset", "out_of_range"));
    }
    if query.tags.len() > 10 {
        errors.push(err("tags", "too_many"));
    }
    let mut seen = Vec::new();
    let mut tags = Vec::new();
    for (i, raw) in query.tags.iter().enumerate() {
        let tag = raw.trim();
        let len = tag.chars().count();
        let field = format!("tags[{i}]");
        if len == 0 {
            errors.push(err(&field, "empty"));
        } else if len > 30 {
            errors.push(err(&field, "too_long"));
        } else if seen.contains(&tag.to_lowercase()) {
            errors.push(err(&field, "duplicate"));
        } else {
            seen.push(tag.to_lowercase());
            tags.push(tag.to_string());
        }
    }
    if errors.is_empty() { Ok(Page { limit, offset, tags }) } else { Err(errors) }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(limit: Option<u32>, offset: Option<u32>, tags: &[&str]) -> ListQuery {
        ListQuery { limit, offset, tags: tags.iter().map(|t| t.to_string()).collect() }
    }

    #[test]
    fn defaults_are_applied() {
        assert_eq!(
            validate_list_query(&query(None, None, &[])),
            Ok(Page { limit: 20, offset: 0, tags: vec![] }),
            "без параметров: limit 20, offset 0"
        );
    }

    #[test]
    fn limits_and_offsets_have_bounds() {
        for bad in [0, 101, 100_000] {
            let errors = validate_list_query(&query(Some(bad), None, &[])).unwrap_err();
            assert_eq!(errors, vec![err("limit", "out_of_range")], "limit {bad} вне 1..=100");
        }
        assert!(validate_list_query(&query(Some(1), None, &[])).is_ok(), "limit 1 допустим");
        assert!(validate_list_query(&query(Some(100), Some(10_000), &[])).is_ok(), "границы 100 и 10000 допустимы");
        assert_eq!(
            validate_list_query(&query(None, Some(10_001), &[])).unwrap_err(),
            vec![err("offset", "out_of_range")],
            "offset 10001 слишком глубокая страница"
        );
    }

    #[test]
    fn tags_are_trimmed_and_checked_individually() {
        let page = validate_list_query(&query(None, None, &["  rust  ", "tokio"])).unwrap();
        assert_eq!(page.tags, vec!["rust".to_string(), "tokio".to_string()], "теги сохраняются обрезанными");
        let errors = validate_list_query(&query(None, None, &["ok", "  ", &"x".repeat(31)])).unwrap_err();
        assert_eq!(errors, vec![err("tags[1]", "empty"), err("tags[2]", "too_long")], "ошибка называет индекс тега");
    }

    #[test]
    fn duplicate_tags_ignore_case() {
        let errors = validate_list_query(&query(None, None, &["Rust", "go", "rust"])).unwrap_err();
        assert_eq!(errors, vec![err("tags[2]", "duplicate")], "повтор помечается у второго вхождения");
    }

    #[test]
    fn too_many_tags() {
        let many: Vec<String> = (0..11).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(validate_list_query(&query(None, None, &refs)).unwrap_err(), vec![err("tags", "too_many")], "11 тегов это много");
    }

    #[test]
    fn everything_wrong_at_once() {
        let errors = validate_list_query(&query(Some(0), Some(99_999), &["a", "A"])).unwrap_err();
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, vec!["limit", "offset", "tags[1]"], "все ошибки сразу, в порядке limit, offset, tags");
    }
}
