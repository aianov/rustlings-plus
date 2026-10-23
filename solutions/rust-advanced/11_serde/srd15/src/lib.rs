use serde::Deserialize;
use std::borrow::Cow;

#[derive(Debug, Deserialize)]
pub struct PostRef<'a> {
    pub title: &'a str,
    #[serde(borrow)]
    pub author: Cow<'a, str>,
    #[serde(borrow)]
    pub body: Cow<'a, str>,
}

pub fn parse_ref(raw: &str) -> Result<PostRef<'_>, serde_json::Error> {
    serde_json::from_str(raw)
}

pub fn parse_many(raw: &str) -> Result<Vec<PostRef<'_>>, serde_json::Error> {
    serde_json::from_str(raw)
}

pub fn titles(raw: &str) -> Result<Vec<&str>, serde_json::Error> {
    Ok(parse_many(raw)?.into_iter().map(|post| post.title).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(raw: &str, piece: &str) -> bool {
        raw.as_bytes().as_ptr_range().contains(&piece.as_ptr())
    }

    #[test]
    fn plain_strings_are_borrowed_from_the_input() {
        let raw = r#"{"title":"hello","author":"ira","body":"short"}"#;
        let post = parse_ref(raw).unwrap();
        assert!(inside(raw, post.title), "title должен быть срезом исходной строки");
        assert!(matches!(post.author, Cow::Borrowed(_)), "author без экранирования не должен копироваться");
        assert!(inside(raw, &post.author));
        assert!(matches!(post.body, Cow::Borrowed(_)), "body тоже");
    }

    #[test]
    fn escaped_strings_become_owned() {
        let raw = r#"{"title":"t","author":"i\"ra","body":"line1\nline2"}"#;
        let post = parse_ref(raw).unwrap();
        assert_eq!(post.author, "i\"ra");
        assert_eq!(post.body, "line1\nline2");
        assert!(matches!(post.body, Cow::Owned(_)), "символ перевода строки в исходнике записан двумя знаками, срезом его не получить");
    }

    #[test]
    fn many_posts_and_titles() {
        let raw = r#"[{"title":"a","author":"x","body":"1"},{"title":"b","author":"y","body":"2"}]"#;
        let posts = parse_many(raw).unwrap();
        assert_eq!(posts.len(), 2);
        assert!(posts.iter().all(|p| matches!(p.author, Cow::Borrowed(_))), "в массиве заимствование тоже работает");
        let titles = titles(raw).unwrap();
        assert_eq!(titles, vec!["a", "b"]);
        assert!(titles.iter().all(|t| inside(raw, t)));
    }

    #[test]
    fn broken_input_is_an_error() {
        assert!(parse_ref(r#"{"title":"t"}"#).is_err());
    }
}
