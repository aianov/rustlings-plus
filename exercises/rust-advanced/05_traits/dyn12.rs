// Хочется писать `"#rust".is_hashtag()` и `result.or_report(&mut errors)`, хотя `str` и `Result` не наши.
// Методы чужим типам добавляют трейтом-расширением: свой трейт, реализованный для чужого типа. Это
// разрешено правилом сироты, потому что трейт наш. Чтобы метод стал виден, трейт должен быть в области
// видимости (здесь он в том же файле).
// `is_hashtag`: строка начинается с `#`, дальше хотя бы один символ и только буквы, цифры, `_`.
// `clip(n)`: не больше n символов (не байтов!), без разрезания букв.
// `or_report(sink)`: для `Ok(v)` вернуть `Some(v)`, для `Err(e)` положить в `sink` текст ошибки и вернуть `None`.
// TODO: Объяви и реализуй оба трейта-расширения.

pub trait StrExt {
    fn is_hashtag(&self) -> bool;
    fn clip(&self, max_chars: usize) -> String;
}

pub trait ResultExt<T> {
    fn or_report(self, sink: &mut Vec<String>) -> Option<T>;
}

fn main() {
    println!("{}", "#rust".is_hashtag());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashtags() {
        assert!("#rust".is_hashtag());
        assert!("#rust_2024".is_hashtag());
        assert!("#привет".is_hashtag(), "буквы не только латинские");
        assert!(!"rust".is_hashtag(), "без решётки");
        assert!(!"#".is_hashtag(), "после решётки должен быть хотя бы один символ");
        assert!(!"#a b".is_hashtag(), "пробел недопустим");
        assert!(!"#a#b".is_hashtag(), "вторая решётка недопустима");
    }

    #[test]
    fn clip_counts_characters() {
        assert_eq!("hello".clip(3), "hel");
        assert_eq!("hi".clip(10), "hi");
        assert_eq!("привет".clip(3), "при", "считаем символы, а не байты");
        assert_eq!(String::from("abc").clip(0), "");
    }

    #[test]
    fn extension_works_on_string_via_deref() {
        let owned = String::from("#tag");
        assert!(owned.is_hashtag());
    }

    #[test]
    fn or_report_collects_errors() {
        let mut errors = Vec::new();
        let a: Option<i32> = "5".parse::<i32>().or_report(&mut errors);
        let b: Option<i32> = "x".parse::<i32>().or_report(&mut errors);
        let c: Option<u8> = "300".parse::<u8>().or_report(&mut errors);
        assert_eq!((a, b, c), (Some(5), None, None));
        assert_eq!(errors.len(), 2, "в журнал попали две ошибки");
        assert!(errors[0].contains("invalid digit"), "текст берётся из Display ошибки: {:?}", errors[0]);
    }

    #[test]
    fn works_for_any_display_error() {
        let mut errors = Vec::new();
        let r: Result<(), String> = Err("boom".to_string());
        assert_eq!(r.or_report(&mut errors), None);
        assert_eq!(errors, vec!["boom"]);
    }
}
