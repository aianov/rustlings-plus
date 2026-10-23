pub trait Describe {
    fn describe(&self) -> String;
}

impl<T: std::fmt::Display> Describe for T {
    fn describe(&self) -> String {
        format!("<{self}>")
    }
}

pub struct Mine;

fn main() {
    println!("{}", 5.describe());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("dyn10.rs");

    #[test]
    fn works_for_many_types() {
        assert_eq!(5.describe(), "<5>");
        assert_eq!(2.5f64.describe(), "<2.5>");
        assert_eq!(String::from("hi").describe(), "<hi>");
        assert_eq!('x'.describe(), "<x>");
        assert_eq!(true.describe(), "<true>");
        assert_eq!("str".describe(), "<str>", "ссылки на строку тоже Display");
    }

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_dyn10_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        std::fs::write(&file, format!("{SRC}\n{extra}\n")).expect("не удалось записать файл");
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
    fn it_is_a_real_blanket_impl() {
        assert!(
            !compiles_with("impl Describe for u8 { fn describe(&self) -> String { String::new() } }"),
            "для u8 можно добавить свою реализацию: значит, общей реализации для всех Display нет, и каждый тип описан отдельно"
        );
    }

    #[test]
    fn types_without_display_are_not_covered() {
        assert!(
            compiles_with("impl Describe for Mine { fn describe(&self) -> String { String::from(\"mine\") } }"),
            "у типа без Display должна быть возможность иметь собственную реализацию"
        );
    }
}
