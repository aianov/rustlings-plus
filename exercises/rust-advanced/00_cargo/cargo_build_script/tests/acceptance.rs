use std::process::Command;

#[test]
fn words_are_cleaned_sorted_and_unique() {
    assert_eq!(
        wordlist::WORDS,
        ["apple", "banana", "cherry", "fig", "pear"],
        "WORDS должен быть отсортирован, без пустых строк, комментариев и повторов"
    );
}

#[test]
fn contains_uses_the_generated_list() {
    assert!(wordlist::contains("pear"), "pear есть в словаре");
    assert!(!wordlist::contains("grape"), "grape в словаре нет");
    assert!(!wordlist::contains("# слова для проверки орфографии, по одному в строке"), "комментарии в словарь не попадают");
}

#[test]
fn build_string_comes_from_the_build_script() {
    assert_eq!(
        wordlist::BUILD,
        "wordlist 0.3.0 (5 words)",
        "BUILD должен быть собран build.rs из версии пакета и числа слов"
    );
}

#[test]
fn binary_prints_build_and_words() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_wordlist") else {
        panic!("бинарник wordlist не собран");
    };
    let out = Command::new(exe).output().expect("не удалось запустить wordlist");
    assert!(out.status.success(), "wordlist завершился с ошибкой");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "wordlist 0.3.0 (5 words)\napple banana cherry fig pear\n",
        "wordlist печатает строку сборки и слова через пробел"
    );
}

#[test]
fn script_does_not_write_into_src() {
    let mut files: Vec<String> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .expect("нет папки src")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    assert_eq!(
        files,
        ["lib.rs", "main.rs"],
        "в src появились лишние файлы: сгенерированное складывают в OUT_DIR, а не в исходники"
    );
}
