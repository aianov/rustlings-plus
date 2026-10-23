// Разбор тысяч постов в секунду, и профилировщик показывает, что половина времени уходит на аллокации строк:
// каждый `title`, `author`, `body` копируется в свой `String`. Если пост живёт недолго, а исходный JSON
// остаётся в памяти, строки можно не копировать, а брать срезами прямо из него. `&str` так умеет сам,
// но в тексте бывают экранированные символы (`\n`, `\"`), и тогда срез из исходника невозможен: там нужен
// настоящий символ, которого в тексте нет. Для этого есть `Cow<'a, str>`: заимствует, пока можно, и копирует,
// когда нельзя. Сейчас все поля `Cow` каждый раз копируются.
// TODO: Заставь `author` и `body` заимствовать из исходной строки. Допиши функции разбора.
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Debug, Deserialize)]
pub struct PostRef<'a> {
    pub title: &'a str,
    pub author: Cow<'a, str>,
    pub body: Cow<'a, str>,
}

pub fn parse_ref(raw: &str) -> Result<PostRef<'_>, serde_json::Error> {
    todo!("один пост из строки")
}

pub fn parse_many(raw: &str) -> Result<Vec<PostRef<'_>>, serde_json::Error> {
    todo!("массив постов из строки")
}

pub fn titles(raw: &str) -> Result<Vec<&str>, serde_json::Error> {
    todo!("только заголовки, без копирования")
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
