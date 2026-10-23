// Журнал хранит значения самых разных типов как `Box<dyn Debug>` и печатает их позже. `level_name` отдаёт
// названия уровней, которые лежат в самой программе. `stash` кладёт значение в журнал, и компилятор
// ругается: «the parameter type `T` may not live long enough». Дело в том, что `Box<dyn Debug>` без
// пометки значит `Box<dyn Debug + 'static>`, а `T` может оказаться ссылкой на что-то временное.
// Важно не перепутать: `T: 'static` не значит «живёт вечно», это значит «не держит коротких ссылок».
// TODO: Допиши `level_name` (0 это debug, 1 info, 2 warn, остальное error) с подходящим результатом и
// условие на `T` в `stash`, чтобы можно было класть `String`, числа и `&'static str`, но не `&локальная`.
use std::fmt::Debug;

pub fn level_name(level: u8) -> &str {
    todo!()
}

pub fn stash<T: Debug>(journal: &mut Vec<Box<dyn Debug>>, value: T) {
    journal.push(Box::new(value));
}

pub fn render(journal: &[Box<dyn Debug>]) -> Vec<String> {
    journal.iter().map(|v| format!("{v:?}")).collect()
}

fn main() {
    let mut journal: Vec<Box<dyn Debug>> = Vec::new();
    stash(&mut journal, level_name(1));
    println!("{:?}", render(&journal));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm8.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn level_names() {
        assert_eq!(level_name(0), "debug");
        assert_eq!(level_name(1), "info");
        assert_eq!(level_name(2), "warn");
        assert_eq!(level_name(200), "error");
    }

    #[test]
    fn journal_keeps_different_types() {
        let mut journal: Vec<Box<dyn Debug>> = Vec::new();
        stash(&mut journal, 42);
        stash(&mut journal, String::from("owned"));
        stash(&mut journal, "literal");
        stash(&mut journal, vec![1, 2]);
        assert_eq!(render(&journal), vec!["42", "\"owned\"", "\"literal\"", "[1, 2]"]);
    }

        #[test]
    fn level_names_live_forever() {
        assert!(compiles_with("pub fn t() -> &'static str { level_name(3) }"), "названия уровней лежат в программе, результат должен быть 'static");
    }

    #[test]
    fn owned_values_are_static_but_borrowed_locals_are_not() {
        assert!(compiles_with("pub fn t() { let mut j: Vec<Box<dyn std::fmt::Debug>> = Vec::new(); let s = String::from(\"x\"); stash(&mut j, s); }"), "владеющая String годится: у неё нет коротких ссылок");
        assert!(compiles_with("pub fn t() { let mut j: Vec<Box<dyn std::fmt::Debug>> = Vec::new(); stash(&mut j, \"x\"); }"), "литерал это &'static str");
        assert!(!compiles_with("pub fn t() { let mut j: Vec<Box<dyn std::fmt::Debug>> = Vec::new(); let s = String::from(\"x\"); stash(&mut j, &s); }"), "ссылка на локальную переменную не 'static: класть её в журнал нельзя");
    }
}
