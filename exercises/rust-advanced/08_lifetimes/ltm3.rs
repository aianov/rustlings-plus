// Счётчик уникальных слов хранит в векторе ссылки на слова из исходного текста, без копий. Функция
// `remember` дописывает слово, если его ещё не было. Сигнатура выглядит безобидно: одно время жизни `'a`
// на всё. Но `unique_words`, которая вызывает её в цикле, не собирается: «cannot move out of `seen`
// because it is borrowed». Заём вектора почему-то не заканчивается после вызова. Подумай, чему именно равно
// `'a` в `&'a mut Vec<&'a str>`.
// TODO: Почини сигнатуру `remember`, чтобы заём вектора кончался вместе с вызовом, а слова оставались ссылками на текст.

pub fn remember<'a>(seen: &'a mut Vec<&'a str>, word: &'a str) -> bool {
    if seen.contains(&word) {
        false
    } else {
        seen.push(word);
        true
    }
}

pub fn unique_words(text: &str) -> Vec<&str> {
    let mut seen = Vec::new();
    for word in text.split_whitespace() {
        remember(&mut seen, word);
    }
    seen
}

fn main() {
    println!("{:?}", unique_words("a b a c"));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm3.rs");

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
    fn keeps_first_occurrences_in_order() {
        assert_eq!(unique_words("to be or not to be"), vec!["to", "be", "or", "not"]);
        assert!(unique_words("   ").is_empty());
    }

    #[test]
    fn words_still_point_into_the_text() {
        let text = String::from("alpha beta alpha");
        let words = unique_words(&text);
        let start = text.as_ptr() as usize;
        assert!(words.iter().all(|w| (start..start + text.len()).contains(&(w.as_ptr() as usize))), "слова должны быть ссылками на исходный текст, а не копиями");
    }

    #[test]
    fn remember_can_be_called_again_and_again() {
        assert!(
            compiles_with("pub fn t() { let mut seen: Vec<&str> = Vec::new(); remember(&mut seen, \"a\"); remember(&mut seen, \"b\"); assert_eq!(seen.len(), 2); }"),
            "после вызова remember вектор должен снова быть доступен: заём `&mut` не должен жить столько же, сколько слова внутри"
        );
        assert!(
            compiles_with("pub fn t<'a>(text: &'a str) -> Vec<&'a str> { let mut seen = Vec::new(); remember(&mut seen, text); seen }"),
            "слова в векторе должны жить столько же, сколько текст, из которого они взяты"
        );
    }
}
