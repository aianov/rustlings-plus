use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
pub struct Post {
    pub id: u64,
    pub title: String,
}

#[derive(Debug, PartialEq)]
pub enum ParseReport {
    Truncated,
    Syntax { line: usize, column: usize },
    Shape { line: usize, column: usize, reason: String },
}

pub fn check(raw: &str) -> Result<Post, ParseReport> {
    use serde_json::error::Category;

    serde_json::from_str(raw).map_err(|e| match e.classify() {
        Category::Eof => ParseReport::Truncated,
        Category::Syntax | Category::Io => ParseReport::Syntax { line: e.line(), column: e.column() },
        Category::Data => {
            let text = e.to_string();
            let reason = text.rsplit_once(" at line ").map_or(text.as_str(), |(head, _)| head).to_string();
            ParseReport::Shape { line: e.line(), column: e.column(), reason }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_post() {
        assert_eq!(check(r#"{"id":1,"title":"hi"}"#), Ok(Post { id: 1, title: "hi".into() }));
    }

    #[test]
    fn cut_off_input_is_truncated() {
        assert_eq!(check(r#"{"id":"#), Err(ParseReport::Truncated));
        assert_eq!(check(r#"{"id":1,"title":"hi"#), Err(ParseReport::Truncated), "оборванная строка тоже обрыв");
        assert_eq!(check(""), Err(ParseReport::Truncated));
    }

    #[test]
    fn not_json_is_syntax_with_a_place() {
        assert_eq!(check(r#"{"id": x}"#), Err(ParseReport::Syntax { line: 1, column: 8 }));
        let multi = "{\n  \"id\": 1,\n  \"title\": @\n}";
        assert_eq!(check(multi), Err(ParseReport::Syntax { line: 3, column: 12 }), "строки считаются с единицы");
        assert!(matches!(check(r#"{"id":1,}"#), Err(ParseReport::Syntax { .. })));
    }

    #[test]
    fn valid_json_of_the_wrong_shape_has_a_clean_reason() {
        match check(r#"{"id":"a","title":"t"}"#) {
            Err(ParseReport::Shape { line, column, reason }) => {
                assert_eq!((line, column), (1, 9));
                assert_eq!(reason, "invalid type: string \"a\", expected u64", "причина без хвоста про строку и столбец");
            }
            other => panic!("ждали Shape, получили {other:?}"),
        }
        match check(r#"{"id":1}"#) {
            Err(ParseReport::Shape { reason, .. }) => assert_eq!(reason, "missing field `title`"),
            other => panic!("ждали Shape, получили {other:?}"),
        }
    }
}
