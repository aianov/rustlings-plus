use validation::{
    CharLen, Email, FieldError, Matches, MaxItems, NotBlank, OneOf, Pattern, PatternError, Range, Rule, Username,
    Validator,
};

fn codes(errors: &[FieldError]) -> Vec<(&str, &'static str)> {
    errors.iter().map(|e| (e.path.as_str(), e.code)).collect()
}

#[test]
fn pattern_matches_whole_strings_with_quantifiers_and_classes() {
    let p = Pattern::parse("[a-z][a-z0-9_]{2,19}").unwrap();
    assert!(p.is_match("alice_1"), "alice_1 подходит");
    assert!(!p.is_match("Alice"), "заглавная не входит в класс");
    assert!(!p.is_match("ab"), "короткая строка");
    let p = Pattern::parse("[^@ ]+@[^@ ]+\\.[a-z]{2,}").unwrap();
    assert!(p.is_match("a.b@mail.example.com") && !p.is_match("a b@x.com"), "классы с отрицанием и экранированная точка");
    assert!(Pattern::parse("a*ab").unwrap().is_match("aaab"), "жадный повтор должен отдавать символы назад");
}

#[test]
fn pattern_reports_syntax_errors() {
    assert_eq!(Pattern::parse("[abc").unwrap_err(), PatternError::Unclosed, "незакрытый класс");
    assert_eq!(Pattern::parse("+a").unwrap_err(), PatternError::NothingToRepeat, "повтор без элемента");
    assert_eq!(Pattern::parse("a{3,2}").unwrap_err(), PatternError::BadQuantifier, "границы наоборот");
    assert_eq!(Pattern::parse("[z-a]").unwrap_err(), PatternError::BadRange, "диапазон наоборот");
}

#[test]
fn pattern_does_not_explode_on_many_stars() {
    let p = Pattern::parse("a*a*a*a*a*a*a*a*b").unwrap();
    let started = std::time::Instant::now();
    assert!(!p.is_match(&"a".repeat(60)), "b в строке нет");
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "сопоставление должно запоминать неудачные состояния");
}

#[test]
fn string_rules_report_codes_and_messages() {
    assert_eq!(NotBlank.check("  ").map(|r| r.0), Some("blank"), "пробелы это пусто");
    assert_eq!(NotBlank.check(" x "), None, "непустое проходит");
    let len = CharLen { min: 3, max: 5 };
    assert_eq!(len.check("ab").map(|r| r.0), Some("too_short"), "короткое");
    assert_eq!(len.check("абвгде").map(|r| r.0), Some("too_long"), "шесть символов длиннее пяти (считаем символы)");
    assert_eq!(len.check("  абв  "), None, "после trim три символа");
    let message = len.check("a").unwrap().1;
    assert!(!message.is_empty(), "сообщение не пустое");
    assert!(OneOf(&["asc", "desc"]).check("up").is_some(), "значение вне списка");
    assert!(OneOf(&["asc", "desc"]).check("asc").is_none(), "значение из списка");
    let rule = Matches::new("[a-z]+").unwrap();
    assert_eq!(rule.check("abc"), None, "подходит под шаблон");
    assert_eq!(rule.check("ab1").map(|r| r.0), Some("invalid_format"), "не подходит под шаблон");
    assert!(Matches::new("[").is_err(), "плохой шаблон сообщает об ошибке при создании правила");
}

#[test]
fn numeric_and_collection_rules() {
    assert_eq!(Range { min: 1u32, max: 100 }.check(&0).map(|r| r.0), Some("out_of_range"), "u32 ниже границы");
    assert_eq!(Range { min: 1u32, max: 100 }.check(&100), None, "граница включена");
    assert_eq!(Range { min: -5i64, max: 5 }.check(&6).map(|r| r.0), Some("out_of_range"), "i64");
    assert!(Range { min: 0.0f64, max: 1.0 }.check(&1.5).is_some(), "f64");
    let tags = vec!["a".to_string(); 3];
    assert_eq!(MaxItems(2).check(&tags[..]).map(|r| r.0), Some("too_many"), "слишком много элементов");
    assert_eq!(MaxItems(3).check(&tags[..]), None, "ровно предел");
}

#[test]
fn validator_collects_errors_with_paths() {
    let mut v = Validator::new();
    v.check("username", "ab", &[&CharLen { min: 3, max: 20 }]);
    v.scope("author", |v| {
        v.check("handle", "x", &[&CharLen { min: 3, max: 20 }]);
        v.check("id", &0u64, &[&Range { min: 1, max: u64::MAX }]);
    });
    let links = ["https://ok.example", "ftp://x", "nope"];
    let url = Matches::new("https?://[^ ]+").unwrap();
    v.each("links", &links, |v, link| v.check("url", *link, &[&url]));
    let errors = v.finish().unwrap_err();
    assert_eq!(
        codes(&errors),
        vec![
            ("username", "too_short"),
            ("author.handle", "too_short"),
            ("author.id", "out_of_range"),
            ("links[1].url", "invalid_format"),
            ("links[2].url", "invalid_format"),
        ],
        "пути: вложенные поля через точку, элементы списка в квадратных скобках, порядок добавления"
    );
    assert!(errors.iter().all(|e| !e.message.is_empty()), "у каждой ошибки есть сообщение");
}

#[test]
fn each_nests_inside_each() {
    let mut v = Validator::new();
    let matrix = [vec!["ok", ""], vec![""]];
    v.each("rows", &matrix, |v, row| v.each("cells", row, |v, cell| v.check("value", *cell, &[&NotBlank])));
    assert_eq!(
        codes(&v.finish().unwrap_err()),
        vec![("rows[0].cells[1].value", "blank"), ("rows[1].cells[0].value", "blank")],
        "вложенность каждый раз добавляет свою часть пути"
    );
}

#[test]
fn only_the_first_failing_rule_per_field_is_reported() {
    let mut v = Validator::new();
    let format = Matches::new("[a-z]+").unwrap();
    v.check("name", "A", &[&CharLen { min: 3, max: 20 }, &format]);
    let errors = v.finish().unwrap_err();
    assert_eq!(codes(&errors), vec![("name", "too_short")], "у поля одна ошибка, от первого сработавшего правила");
}

#[test]
fn custom_errors_and_success() {
    let mut v = Validator::new();
    v.scope("poll", |v| v.error("options", "bad_count", "нужно от двух до четырёх вариантов"));
    assert_eq!(codes(&v.finish().unwrap_err()), vec![("poll.options", "bad_count")], "своя ошибка с путём");
    let mut v = Validator::new();
    v.check("ok", "fine", &[&NotBlank]);
    assert_eq!(v.finish(), Ok(()), "без ошибок finish даёт Ok");
}

#[test]
fn newtypes_are_created_only_valid() {
    assert_eq!(Username::parse("  Alice_9 ").unwrap().as_str(), "alice_9", "ник нормализуется");
    assert_eq!(Username::parse("9alice").unwrap_err().code, "invalid_format", "начинается с цифры");
    assert_eq!(Username::parse("ab").unwrap_err().code, "too_short", "короткий");
    assert_eq!(Email::parse(" A+b@Mail.Example.COM ").unwrap().as_str(), "a+b@mail.example.com", "почта нормализуется");
    for bad in ["", "a", "a@b", "@x.y", "a@@x.y", "a b@x.y", "a@x..y"] {
        assert_eq!(Email::parse(bad).unwrap_err().code, "invalid_format", "{bad:?} не адрес");
    }
    assert_eq!(Username::parse("bob").unwrap().to_string(), "bob", "Display");
}
