// Модуль `parser` отдаёт наружу токены, но сам тип токена у него приватный. Вызывающий получает вектор
// значений, имени которых не может написать, а компилятор в новых версиях об этом предупреждает
// (`private_interfaces`, `private_bounds`). Тесты собирают файл с этими предупреждениями, повышенными
// до ошибок, и заодно проверяют, что через API всё ещё можно работать.
// TODO: Исправь так, чтобы публичные сигнатуры не упоминали приватных типов. Как именно, решай сам:
// главное, чтобы `tokenize` и `count` продолжали делать своё дело, а токены печатались через `{}`
// (то есть нужен `impl Display`).

pub mod parser {
    #[derive(Debug, Clone, PartialEq)]
    struct Token {
        text: String,
    }

    trait Source {
        fn text(&self) -> &str;
    }

    impl Source for &str {
        fn text(&self) -> &str {
            self
        }
    }

    pub fn tokenize(src: &str) -> Vec<Token> {
        src.split(|c: char| c.is_whitespace() || c == '+')
            .filter(|s| !s.is_empty())
            .map(|s| Token { text: s.to_string() })
            .collect()
    }

    pub fn count<S: Source>(source: S) -> usize {
        tokenize(source.text()).len()
    }
}

fn main() {
    println!("{}", parser::count("1 + 22"));
}

#[cfg(test)]
mod tests {
    use super::*;

const SRC: &str = include_str!("wsp4.rs");

fn rustc(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("rustc")
        .args(args)
        .output()
        .expect("не удалось запустить rustc: он должен быть в PATH");
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn scratch_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rustlings_probe_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("не удалось создать временную папку");
    dir
}

fn inside(extra: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let file = dir.join("probe.rs");
    std::fs::write(&file, format!("{SRC}\n{extra}\n")).expect("не удалось записать файл");
    let result = rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "probe", "--emit=metadata",
        "-A", "warnings", "-D", "private_interfaces", "-D", "private_bounds",
        "--out-dir", dir.to_str().expect("путь не UTF-8"), file.to_str().expect("путь не UTF-8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn outside(client: &str) -> Result<(), String> {
    let dir = scratch_dir();
    let lib = dir.join("exercise.rs");
    std::fs::write(&lib, SRC).expect("не удалось записать файл");
    let dir_s = dir.to_str().expect("путь не UTF-8");
    rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "exercise", "-A", "warnings",
        "--out-dir", dir_s, lib.to_str().expect("путь не UTF-8"),
    ])
    .map_err(|e| format!("сам файл не собирается как библиотека:\n{e}"))?;
    let bin = dir.join("client.rs");
    std::fs::write(&bin, client).expect("не удалось записать файл");
    let rlib = dir.join("libexercise.rlib");
    let result = rustc(&[
        "--edition", "2024", "--crate-type", "lib", "--crate-name", "client", "--emit=metadata", "-A", "warnings",
        "--extern", &format!("exercise={}", rlib.to_str().expect("путь не UTF-8")),
        "--out-dir", dir_s, bin.to_str().expect("путь не UTF-8"),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

    #[test]
    fn tokens_are_printable() {
        let tokens: Vec<String> = parser::tokenize("1 + 22 +foo").iter().map(|t| t.to_string()).collect();
        assert_eq!(tokens, ["1", "22", "foo"], "каждый токен должен печататься как его текст");
    }

    #[test]
    fn count_works_on_plain_strings() {
        assert_eq!(parser::count("a b c"), 3, "в строке три токена");
        assert_eq!(parser::count(""), 0, "пустая строка без токенов");
    }

    #[test]
    fn public_signatures_do_not_mention_private_types() {
        if let Err(e) = inside("") {
            panic!("компилятор жалуется на утечку приватных типов в публичном интерфейсе:\n{e}");
        }
    }

    #[test]
    fn an_outside_crate_can_use_the_api() {
        assert!(
            outside(
                "pub fn f() -> (usize, Vec<String>) { (exercise::parser::count(\"a b\"), exercise::parser::tokenize(\"a b\").iter().map(|t| t.to_string()).collect()) }"
            )
            .is_ok(),
            "внешний крейт должен вызывать count и tokenize и печатать токены"
        );
    }
}
