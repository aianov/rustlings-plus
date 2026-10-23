#![allow(dead_code)]

use describe_derive::Describe;
use std::collections::HashMap;

#[derive(Describe)]
pub struct Profile {
    pub id: u64,
    /// Логин пользователя.
    pub login: String,
    pub(crate) tags: Vec<String>,
}

#[derive(Describe)]
#[allow(clippy::type_complexity)]
struct Tricky {
    pub map: HashMap<String, Vec<(u8, u8)>>,
    callback: fn(u8) -> Result<u8, String>,
    matrix: [[u8; 2]; 3],
    reference: &'static str,
    boxed: Box<dyn Fn(u32) -> u32 + Send>,
    #[allow(unused)]
    r#type: u8,
    last: Option<HashMap<u8, u8>>,
}

#[derive(Describe)]
struct Pair(u8, String,);

#[derive(Describe)]
struct Wrapper(HashMap<String, Vec<u8>>);

#[derive(Describe)]
struct Marker;

#[derive(Describe)]
struct Empty {}

#[derive(Describe)]
#[doc = "Реакция на пост."]
pub(crate) enum Reaction {
    Like,
    /// С картинкой.
    Emoji(String),
    #[allow(unused)]
    Custom { code: u32, label: String },
}

#[derive(Describe)]
#[repr(u8)]
enum Level {
    Low = 1,
    High = 10,
}

#[test]
fn named_struct() {
    assert_eq!(Profile::TYPE_NAME, "Profile");
    assert_eq!(Profile::FIELDS, ["id", "login", "tags"]);
    assert_eq!(Profile::describe(), "Profile { id, login, tags }");
}

#[test]
fn types_with_commas_and_arrows_do_not_confuse_the_field_split() {
    assert_eq!(Tricky::FIELDS, ["map", "callback", "matrix", "reference", "boxed", "r#type", "last"]);
    assert_eq!(Tricky::FIELDS.len(), 7, "запятые внутри <..> и стрелки -> не разделители полей");
}

#[test]
fn tuple_structs_use_indices() {
    assert_eq!(Pair::FIELDS, ["0", "1"]);
    assert_eq!(Pair::describe(), "Pair(0, 1)");
    assert_eq!(Wrapper::FIELDS, ["0"], "одно поле с запятыми внутри типа");
    assert_eq!(Wrapper::describe(), "Wrapper(0)");
}

#[test]
fn unit_and_empty_structs() {
    assert_eq!(Marker::FIELDS.len(), 0);
    assert_eq!(Marker::describe(), "Marker");
    assert_eq!(Empty::FIELDS.len(), 0);
    assert_eq!(Empty::describe(), "Empty");
    assert_eq!(Empty::TYPE_NAME, "Empty");
}

#[test]
fn enums_list_their_variants() {
    assert_eq!(Reaction::TYPE_NAME, "Reaction");
    assert_eq!(Reaction::VARIANTS, ["Like", "Emoji", "Custom"]);
    assert_eq!(Reaction::describe(), "Reaction = Like | Emoji | Custom");
    assert_eq!(Level::VARIANTS, ["Low", "High"], "значения вариантов (= 1) в результат не попадают");
}

#[test]
fn constants_are_usable_at_compile_time() {
    const COUNT: usize = Profile::FIELDS.len();
    static NAMES: [&str; 2] = [Profile::TYPE_NAME, Reaction::TYPE_NAME];
    assert_eq!(COUNT, 3);
    assert_eq!(NAMES, ["Profile", "Reaction"]);
}

#[test]
fn derived_types_still_work_as_ordinary_types() {
    let profile = Profile { id: 1, login: "ann".into(), tags: vec![] };
    assert_eq!((profile.id, profile.login.as_str()), (1, "ann"));
    assert_eq!(Level::High as u8, 10);
}
