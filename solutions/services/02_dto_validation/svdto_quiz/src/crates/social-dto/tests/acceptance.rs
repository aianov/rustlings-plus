use social_dto::{Link, NewPost, Profile, apply_profile_patch, parse_post, parse_register};

fn pairs(errors: &[validation::FieldError]) -> Vec<(&str, &'static str)> {
    errors.iter().map(|e| (e.path.as_str(), e.code)).collect()
}

fn profile() -> Profile {
    Profile { display_name: "Alice".to_string(), bio: Some("люблю Rust".to_string()), birth_year: Some(1999) }
}

#[test]
fn registration_is_parsed_and_normalized() {
    let reg = parse_register(
        r#"{"username":"  Alice_1 ","email":" Alice@Example.COM ","displayName":" Анна \u200b  Мария ","birthYear":1999,"marketingOptIn":true}"#,
    )
    .unwrap();
    assert_eq!(reg.username.as_str(), "alice_1", "ник нормализован");
    assert_eq!(reg.email.as_str(), "alice@example.com", "почта нормализована");
    assert_eq!(reg.display_name.as_deref(), Some("Анна Мария"), "имя: невидимые символы убраны, пробелы схлопнуты");
    assert_eq!((reg.birth_year, reg.marketing_opt_in), (Some(1999), true), "остальные поля");
}

#[test]
fn registration_optional_fields_and_defaults() {
    let reg = parse_register(r#"{"username":"bob","email":"bob@x.io"}"#).unwrap();
    assert_eq!((reg.display_name, reg.birth_year, reg.marketing_opt_in), (None, None, false), "умолчания");
}

#[test]
fn registration_reports_all_problems_in_field_order() {
    let errors = parse_register(r#"{"username":"a","email":"nope","displayName":"  ","birthYear":1800}"#).unwrap_err();
    assert_eq!(
        pairs(&errors),
        vec![
            ("username", "too_short"),
            ("email", "invalid_format"),
            ("displayName", "too_short"),
            ("birthYear", "out_of_range"),
        ],
        "все четыре ошибки, у каждой путь и код"
    );
    assert!(errors.iter().all(|e| !e.message.is_empty()), "у каждой ошибки есть сообщение");
}

#[test]
fn registration_rejects_unknown_fields_and_garbage() {
    let errors = parse_register(r#"{"username":"bob","email":"bob@x.io","role":"admin","isAdmin":true}"#).unwrap_err();
    assert_eq!(pairs(&errors), vec![("isAdmin", "unknown_field"), ("role", "unknown_field")], "лишние поля названы (по алфавиту)");
    for bad in ["{oops", "[]", r#"{"email":"a@b.c"}"#, r#"{"username":"a","email":"b@c.d","birthYear":"1999"}"#] {
        let errors = parse_register(bad).unwrap_err();
        assert_eq!(pairs(&errors), vec![("", "bad_json")], "{bad}: битое тело это одна ошибка bad_json с пустым путём");
    }
}

#[test]
fn patch_changes_only_what_was_sent() {
    let mut p = profile();
    apply_profile_patch(&mut p, r#"{"bio":"  новое  "}"#).unwrap();
    assert_eq!(p.bio.as_deref(), Some("новое"), "bio обрезано и записано");
    assert_eq!((p.display_name.as_str(), p.birth_year), ("Alice", Some(1999)), "остальное не тронуто");
}

#[test]
fn patch_null_clears_where_allowed() {
    let mut p = profile();
    apply_profile_patch(&mut p, r#"{"bio":null,"birthYear":null}"#).unwrap();
    assert_eq!((p.bio, p.birth_year), (None, None), "null очищает bio и birthYear");
    let mut p = profile();
    let errors = apply_profile_patch(&mut p, r#"{"displayName":null}"#).unwrap_err();
    assert_eq!(pairs(&errors), vec![("displayName", "null_not_allowed")], "имя очистить нельзя");
    assert_eq!(p, profile(), "профиль не изменился");
}

#[test]
fn patch_is_all_or_nothing_and_checks_values() {
    let mut p = profile();
    let errors = apply_profile_patch(&mut p, r#"{"displayName":"Боб","bio":"","birthYear":3000}"#).unwrap_err();
    assert_eq!(pairs(&errors), vec![("bio", "too_short"), ("birthYear", "out_of_range")], "две ошибки, имя в порядке");
    assert_eq!(p, profile(), "хорошее поле из плохого запроса тоже не применилось");
    let errors = apply_profile_patch(&mut p, r#"{"role":"admin"}"#).unwrap_err();
    assert_eq!(pairs(&errors), vec![("role", "unknown_field")], "mass assignment: лишнее поле отклоняется");
}

#[test]
fn empty_patch_changes_nothing() {
    let mut p = profile();
    apply_profile_patch(&mut p, "{}").unwrap();
    assert_eq!(p, profile(), "пустой PATCH ничего не меняет");
}

#[test]
fn post_is_normalized() {
    let post = parse_post(
        r##"{"text":"  привет, мир ","tags":[" #Rust Lang","Tokio"],"links":[{"url":" https://a.example/x ","title":"пример"},{"url":"http://b.example"}]}"##,
    )
    .unwrap();
    assert_eq!(
        post,
        NewPost {
            text: "привет, мир".to_string(),
            tags: vec!["rust-lang".to_string(), "tokio".to_string()],
            links: vec![
                Link { url: "https://a.example/x".to_string(), title: Some("пример".to_string()) },
                Link { url: "http://b.example".to_string(), title: None },
            ],
        },
        "текст обрезан, теги нормализованы, ссылки обрезаны"
    );
    assert_eq!(parse_post(r#"{"text":"только текст"}"#).unwrap().tags.len(), 0, "теги и ссылки необязательны");
}

#[test]
fn post_errors_carry_paths() {
    let json = format!(
        r#"{{"text":"{}","tags":["a","A","", "{}"],"links":[{{"url":"ftp://x"}},{{"url":"https://ok.example","title":"{}"}}]}}"#,
        "я".repeat(281),
        "т".repeat(31),
        "п".repeat(51)
    );
    let errors = parse_post(&json).unwrap_err();
    assert_eq!(
        pairs(&errors),
        vec![
            ("text", "too_long"),
            ("tags[1]", "duplicate"),
            ("tags[2]", "too_short"),
            ("tags[3]", "too_long"),
            ("links[0].url", "invalid_format"),
            ("links[1].title", "too_long"),
        ],
        "пути вида tags[1] и links[0].url, порядок text, tags, links"
    );
}

#[test]
fn post_collection_limits() {
    let tags: Vec<String> = (0..11).map(|i| format!("\"t{i}\"")).collect();
    let errors = parse_post(&format!(r#"{{"text":"x","tags":[{}]}}"#, tags.join(","))).unwrap_err();
    assert_eq!(pairs(&errors), vec![("tags", "too_many")], "11 тегов это много");
    let links: Vec<String> = (0..6).map(|i| format!(r#"{{"url":"https://e{i}.example"}}"#)).collect();
    let errors = parse_post(&format!(r#"{{"text":"x","links":[{}]}}"#, links.join(","))).unwrap_err();
    assert_eq!(pairs(&errors), vec![("links", "too_many")], "шесть ссылок это много");
    assert_eq!(pairs(&parse_post(r#"{"text":"x","author":1}"#).unwrap_err()), vec![("author", "unknown_field")], "лишнее поле");
}
