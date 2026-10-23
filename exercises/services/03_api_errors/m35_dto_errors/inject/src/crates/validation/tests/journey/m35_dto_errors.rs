use std::time::{Duration, Instant};

use validation::{
    CharLen, HttpUrl, Invalid, Matches, MaxItems, NoControl, NotBlank, OneOf, Pattern,
    PatternError, Range, Rule, RuleExt, Unique, Validator, Violation, collapse_spaces,
};

fn code<T: ?Sized>(rule: &impl Rule<T>, value: &T) -> Option<&'static str> {
    rule.check(value).err().map(|violation| violation.code)
}

#[test]
fn rules_answer_with_a_stable_code() {
    assert_eq!(code(&NotBlank, "  \t"), Some("blank"));
    assert_eq!(code(&NotBlank, " a "), None);
    let length = CharLen { min: 3, max: 5 };
    assert_eq!(
        code(&length, "ёжик"),
        None,
        "длина в символах: «ёжик» это 4 символа и 8 байт"
    );
    assert_eq!(code(&length, "ёж"), Some("too_short"));
    assert_eq!(code(&length, "ёжики!"), Some("too_long"));
    assert_eq!(code(&Range { min: 1, max: 100 }, &0), Some("out_of_range"));
    assert_eq!(code(&Range { min: 1, max: 100 }, &100), None);
    assert_eq!(code(&MaxItems(2), [1, 2, 3].as_slice()), Some("too_many"));
    assert_eq!(code(&Unique, ["a", "b", "a"].as_slice()), Some("duplicate"));
    assert_eq!(code(&Unique, ["a", "b"].as_slice()), None);
    assert_eq!(
        code(&OneOf(&["bio", "display_name"]), "admin"),
        Some("not_allowed")
    );
    assert_eq!(code(&NoControl, "строка\r\nвторая"), Some("control_char"));
    assert_eq!(code(&NoControl, "просто текст"), None);
    let message = length.check("ab").expect_err("коротко").message;
    assert!(message.contains('3'), "сообщение для человека: {message}");
}

#[test]
fn urls_are_http_with_a_host() {
    for good in [
        "https://example.com",
        "http://example.com:8080/path?q=1#top",
        "HTTPS://Docs.RS/tokio",
    ] {
        assert_eq!(code(&HttpUrl, good), None, "{good}");
    }
    for bad in [
        "javascript:alert(1)",
        "ftp://example.com",
        "https://",
        "https:// example.com",
        "https://user:pass@example.com",
        "https://example.com:99999",
        "https://exa..mple.com",
        "example.com",
    ] {
        assert_eq!(code(&HttpUrl, bad), Some("invalid_url"), "{bad}");
    }
    let huge = format!("https://example.com/{}", "a".repeat(3000));
    assert_eq!(code(&HttpUrl, huge.as_str()), Some("too_long"));
}

#[test]
fn rules_combine_and_closures_are_rules_too() {
    let even = |value: &u32| {
        if value % 2 == 0 {
            Ok(())
        } else {
            Err(Violation::new("odd", "нужно чётное"))
        }
    };
    let rule = Range { min: 1, max: 10 }.and(even);
    assert_eq!(
        code(&rule, &12),
        Some("out_of_range"),
        "первое нарушение побеждает"
    );
    assert_eq!(code(&rule, &7), Some("odd"));
    assert_eq!(code(&rule, &8), None);
    let name = NotBlank.and(CharLen { min: 2, max: 4 });
    assert_eq!(
        code(&name, "   "),
        Some("blank"),
        "порядок правил это порядок проверок"
    );
}

#[test]
fn the_mini_pattern_matches_whole_strings() {
    let tag = Pattern::parse("[a-z0-9_]+").expect("шаблон разбирается");
    assert!(tag.is_match("rust_2024"));
    assert!(
        !tag.is_match("Rust"),
        "шаблон целиком, а не поиск подстроки"
    );
    assert!(!tag.is_match(""));
    let phone = Pattern::parse("\\+?[0-9]{10,12}").expect("шаблон");
    assert!(phone.is_match("+79991234567"));
    assert!(!phone.is_match("12345"));
    let any = Pattern::parse("a.c").expect("точка");
    assert!(
        any.is_match("abc") && any.is_match("aжc"),
        "точка это любой символ, а не байт"
    );
    assert_eq!(Pattern::parse("[a-").err(), Some(PatternError::Unclosed));
    assert_eq!(
        Pattern::parse("*a").err(),
        Some(PatternError::NothingToRepeat)
    );
    assert_eq!(Pattern::parse("[z-a]").err(), Some(PatternError::BadRange));
    assert_eq!(
        Pattern::parse("a{3,1}").err(),
        Some(PatternError::BadQuantifier)
    );
    let greedy = Pattern::parse("a*a*a*a*a*a*a*a*b").expect("шаблон");
    let started = Instant::now();
    assert!(!greedy.is_match(&"a".repeat(200)));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "перебор с запоминанием провалов, а не экспонента"
    );
}

#[test]
fn a_broken_pattern_rejects_instead_of_letting_everything_through() {
    let broken = Matches {
        pattern: "[a-",
        what: "что-то",
    };
    assert_eq!(code(&broken, "anything"), Some("bad_pattern"));
    let tag = Matches {
        pattern: "[a-z]+",
        what: "строчные буквы",
    };
    assert_eq!(code(&tag, "Rust"), Some("invalid_format"));
    assert!(
        tag.check("Rust")
            .expect_err("заглавная")
            .message
            .contains("строчные буквы"),
        "сообщение говорит, что ждали"
    );
}

#[derive(Debug, PartialEq)]
struct Slug(String);

impl TryFrom<&str> for Slug {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<Slug, Violation> {
        let slug = raw.trim().to_lowercase();
        CharLen { min: 2, max: 10 }.check(slug.as_str())?;
        Ok(Slug(slug))
    }
}

struct Link {
    url: &'static str,
    title: &'static str,
}

#[test]
fn the_validator_collects_everything_with_paths() {
    let links = [
        Link {
            url: "https://example.com",
            title: "ok",
        },
        Link {
            url: "ftp://nope",
            title: "ok",
        },
        Link {
            url: "javascript:x",
            title: "строка\nвторая",
        },
    ];
    let mut check = Validator::new();
    let slug = check.parse::<Slug>("slug", "  Rust ");
    check.check("text", "", &NotBlank);
    check.nested("author", |check| {
        check.check("name", "x", &CharLen { min: 2, max: 20 });
        check.each("links", &links, |check, link| {
            check.check("url", link.url, &HttpUrl);
            check.check("title", link.title, &NoControl);
        });
    });
    assert!(!check.is_clean());
    assert_eq!(
        slug,
        Some(Slug("rust".to_string())),
        "нормализация до проверки"
    );
    let invalid = check.finish().expect_err("ошибок много");
    assert_eq!(
        invalid.pairs(),
        [
            ("text", "blank"),
            ("author.name", "too_short"),
            ("author.links[1].url", "invalid_url"),
            ("author.links[2].url", "invalid_url"),
            ("author.links[2].title", "control_char"),
        ],
        "все ошибки разом, у каждой путь до поля"
    );
    assert!(
        invalid.to_string().contains("author.links[2].title"),
        "{invalid}"
    );
}

#[test]
fn finish_with_returns_the_value_only_when_clean() {
    let mut clean = Validator::new();
    let slug = clean.parse::<Slug>("slug", "ok");
    assert_eq!(clean.finish_with(slug), Ok(Slug("ok".to_string())));
    let mut dirty = Validator::new();
    let slug = dirty.parse::<Slug>("slug", "x");
    let failed: Result<Slug, Invalid> = dirty.finish_with(slug);
    assert_eq!(
        failed.expect_err("короткий").pairs(),
        [("slug", "too_short")]
    );
    assert_eq!(collapse_spaces("  Анна \t  Каренина \n"), "Анна Каренина");
}
