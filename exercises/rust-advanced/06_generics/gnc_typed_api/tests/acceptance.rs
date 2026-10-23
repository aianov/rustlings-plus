use std::collections::HashMap;
use typed_api::*;

#[test]
fn ids_are_copy_eq_and_hash_for_any_marker() {
    let id: Id<Post> = Id::new(7);
    let copy = id;
    assert_eq!(id, copy, "Id копируется и сравнивается по числу");
    let mut likes: HashMap<Id<Post>, u32> = HashMap::new();
    *likes.entry(id).or_default() += 1;
    *likes.entry(copy).or_default() += 1;
    assert_eq!(likes[&id], 2, "один и тот же номер это один ключ");
    assert_eq!(format!("{id:?}"), "Id(7)");
    fn is_send<T: Send + Sync>() {}
    is_send::<Id<std::rc::Rc<u8>>>();
}

#[test]
fn ids_print_and_parse_with_their_prefix() {
    assert_eq!(Id::<User>::new(42).to_string(), "u_42");
    assert_eq!(Id::<Post>::new(7).to_string(), "p_7");
    assert_eq!("u_42".parse::<Id<User>>(), Ok(Id::new(42)));
    assert_eq!("p_7".parse::<Id<User>>(), Err(IdError::WrongPrefix { expected: "u" }), "чужой префикс это ошибка, а не молчаливая подмена");
    assert_eq!("42".parse::<Id<User>>(), Err(IdError::WrongPrefix { expected: "u" }));
    assert!(matches!("u_x".parse::<Id<User>>(), Err(IdError::BadNumber(_))), "после префикса должно быть число");
}

#[test]
fn money_arithmetic_and_display() {
    assert_eq!(Money(150) + Money(275), Money(425));
    assert_eq!(Money(199) * 3, Money(597));
    let total: Money = [Money(100), Money(250)].into_iter().sum();
    assert_eq!(total, Money(350));
    assert_eq!(Money(500).checked_sub(Money(200)), Some(Money(300)));
    assert_eq!(Money(100).checked_sub(Money(200)), None, "денег меньше нуля не бывает");
    assert_eq!(Money(1234).to_string(), "12.34");
    assert_eq!(Money(5).to_string(), "0.05");
}

#[test]
fn table_hands_out_typed_ids() {
    let mut users: Table<String> = Table::new();
    let ann = users.insert("ann".to_string());
    let bob = users.insert("bob".to_string());
    assert_ne!(ann, bob);
    assert_eq!(users.get(ann).map(String::as_str), Some("ann"));
    assert_eq!(users.get(bob).map(String::as_str), Some("bob"));
    assert_eq!(users.get(Id::new(99)), None);
    assert_eq!(users.get(Id::new(0)), None, "нулевого номера не выдавали");
}

struct Note;

impl Kind for Note {
    const PREFIX: &'static str = "n";
}

#[test]
fn table_rows_are_found_by_query_parameter() {
    let mut notes: Table<Note> = Table::new();
    let first = notes.insert(Note);
    assert!(notes.lookup(&format!("page=1&note={first}"), "note").is_ok_and(|row| row.is_some()), "id из строки запроса находит строку таблицы");
    assert!(notes.lookup("page=1", "note").is_ok_and(|row| row.is_none()), "нет ключа это не ошибка");
    assert!(notes.lookup("note=n_77", "note").is_ok_and(|row| row.is_none()), "неизвестный номер даёт None");
    let err = notes.lookup("note=u_1", "note").err().expect("id пользователя таблице заметок не подходит");
    assert_eq!(err.key, "note");
}

#[test]
fn draft_builds_only_when_complete_in_any_order() {
    let author = Id::<User>::new(1);
    let a = PostDraft::new().author(author).text("hello").build();
    let b = PostDraft::new().text("hello").boost(Money(500)).author(author).build();
    assert_eq!(a, Some(NewPost { author, text: "hello".into(), boost: None }));
    assert_eq!(b.map(|p| p.boost), Some(Some(Money(500))));
}

fn compiles_with(extra: &str) -> bool {
    let dir = std::env::temp_dir().join(format!("typed_api_probe_{}_{}", std::process::id(), extra.len()));
    std::fs::create_dir_all(&dir).expect("папка для пробы");
    let file = dir.join("probe.rs");
    std::fs::write(&file, format!("{}
{extra}
", include_str!("../src/lib.rs"))).expect("запись пробы");
    let ok = std::process::Command::new("rustc")
        .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
        .arg(&dir)
        .arg(&file)
        .output()
        .expect("rustc запускается")
        .status
        .success();
    let _ = std::fs::remove_dir_all(&dir);
    ok
}

#[test]
fn draft_without_required_fields_does_not_compile() {
    assert!(compiles_with("fn p() { let _ = PostDraft::new().author(Id::<User>::new(1)).text(\"t\").build(); }"), "полный черновик должен собираться");
    assert!(!compiles_with("fn p() { let _ = PostDraft::new().author(Id::<User>::new(1)).build(); }"), "build без текста не должен собираться");
    assert!(!compiles_with("fn p() { let _ = PostDraft::new().text(\"t\").build(); }"), "build без автора не должен собираться");
    assert!(!compiles_with("fn p(t: &Table<String>) { t.get(Id::<User>::new(1)); }"), "id пользователя не подходит таблице строк");
}

#[test]
fn params_are_parsed_through_fromstr() {
    let q = "page=3&ratio=0.5&author=u_9&flag=yes";
    assert_eq!(param::<u32>(q, "page"), Ok(Some(3)));
    assert_eq!(param::<f64>(q, "ratio"), Ok(Some(0.5)));
    assert_eq!(param::<Id<User>>(q, "author"), Ok(Some(Id::new(9))), "свой тип с FromStr работает так же");
    assert_eq!(param::<u32>(q, "missing"), Ok(None), "нет ключа это не ошибка");
    let err = param::<bool>(q, "flag").expect_err("yes это не bool");
    assert_eq!(err.key, "flag");
    assert!(!err.reason.is_empty(), "причина берётся из ошибки разбора");
    let wrong = param::<Id<Post>>(q, "author").expect_err("префикс не тот");
    assert!(wrong.reason.contains("p_"), "причина из IdError: {}", wrong.reason);
}
