use feed_app::{register, rename, shadowing, User};
use social_macros::Invalid;

#[test]
fn a_valid_user_registers() {
    assert_eq!(register("ann_7", 20), Ok(User { login: "ann_7".to_string(), age: 20 }));
}

#[test]
fn require_reports_the_field_and_the_reason() {
    assert_eq!(register("ann", 9), Err(Invalid { field: "age", reason: "нужно минимум 13 лет".to_string() }));
}

#[test]
fn valid_login_rejects_bad_logins_with_their_text() {
    let err = register("bad name", 30).unwrap_err();
    assert_eq!(err.field, "login");
    assert!(err.reason.contains("\"bad name\""), "{}", err.reason);
    assert!(register("", 30).is_err());
    assert!(register(&"x".repeat(21), 30).is_err());
}

#[test]
fn macros_work_by_their_full_path() {
    assert_eq!(rename("ann", "bob"), Ok("bob".to_string()));
    assert_eq!(rename("ann", "ann").unwrap_err().field, "login");
    assert!(rename("ann", "no good").is_err());
}

#[test]
fn macros_survive_a_module_with_a_clashing_invalid_type() {
    assert_eq!(shadowing::nickname_ok("zoe"), Ok(3));
    assert!(shadowing::nickname_ok("z z").is_err());
    let _ = shadowing::unused();
}

#[test]
fn internals_stay_private() {
    let source = include_str!("../../social-macros/src/lib.rs");
    assert!(!source.contains("pub mod errors") && !source.contains("pub mod validate"), "модули errors и validate публичными делать нельзя");
    assert!(source.contains("doc(hidden)"), "служебный модуль __private должен быть скрыт из документации");
}

#[test]
fn valid_login_is_an_expression_of_type_result_string() {
    let ok: Result<String, Invalid> = social_macros::valid_login!("fine_name");
    assert_eq!(ok.as_deref(), Ok("fine_name"));
    let bad: Result<String, Invalid> = social_macros::valid_login!(String::from("!"));
    assert!(bad.is_err());
}
