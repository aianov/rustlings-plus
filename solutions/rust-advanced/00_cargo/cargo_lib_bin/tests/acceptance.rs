use std::io::Write;
use std::process::{Command, Stdio};

fn run(exe: &str, input: &str) -> String {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("не удалось запустить бинарник");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "бинарник завершился с ошибкой: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|_| panic!("нет файла {path}"))
}

fn declares_mod(src: &str) -> bool {
    src.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("mod ") || l.starts_with("pub mod ")
    })
}

#[test]
fn library_counts() {
    let s = word_count::stats::analyze("one two\nthree\n");
    assert_eq!(s.lines, 2, "в тексте две строки");
    assert_eq!(s.words, 3, "в тексте три слова");
    assert_eq!(s.bytes, 14, "в тексте 14 байт");
}

#[test]
fn main_binary_counts_words() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_word-count") else {
        panic!("бинарник word-count не собран");
    };
    assert_eq!(run(exe, "one two\nthree\n"), "words: 3", "word-count должен печатать `words: 3`");
    assert_eq!(run(exe, ""), "words: 0", "на пустом вводе должно быть `words: 0`");
}

#[test]
fn second_binary_counts_lines() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_lines") else {
        panic!("нет бинарника lines: создай файл src/bin/lines.rs, cargo найдёт его сам");
    };
    assert_eq!(run(exe, "a\nb\nc\n"), "lines: 3", "lines должен печатать `lines: 3`");
    assert_eq!(run(exe, ""), "lines: 0", "на пустом вводе должно быть `lines: 0`");
}

#[test]
fn binaries_use_the_library_instead_of_copying_modules() {
    for path in ["src/main.rs", "src/bin/lines.rs"] {
        assert!(
            !declares_mod(&source(path)),
            "{path} объявляет свой `mod`: бинарник должен вызывать библиотеку по имени пакета (word_count::...)"
        );
    }
}
