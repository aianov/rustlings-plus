use std::process::Command;

#[test]
fn library_summary_uses_library_type() {
    let books = [shelf::model::Book::new("A", 10), shelf::model::Book::new("B", 5)];
    assert_eq!(shelf::summary(&books), "2 books, 15 pages", "summary должна считать книги и страницы");
}

#[test]
fn binary_prints_summary() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_shelf") else {
        panic!("бинарник shelf не собрался");
    };
    let out = Command::new(exe).output().unwrap();
    assert!(out.status.success(), "shelf завершился с ошибкой");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "2 books, 1016 pages",
        "shelf должен напечатать сводку по двум книгам"
    );
}

#[test]
fn binary_does_not_compile_its_own_copy_of_the_model() {
    let main_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).unwrap();
    assert!(
        !main_rs.lines().any(|l| l.trim_start().starts_with("mod ")),
        "src/main.rs объявляет свой `mod`: это второй экземпляр модуля, а типы из него не совместимы с библиотечными"
    );
}
