// Формат события, которое видят чужие сервисы, меняют случайно: переименовали поле, добавили лишнее, поменяли порядок.
// Тест сверяет свежий вывод с образцом из файла (золотой файл). Цель: `check_golden` читает `golden/<имя>.txt` из каталога
// и сравнивает с `actual`. Перевод строк и пустая строка в конце не считаются отличием (в репозитории файлы бывают с CRLF).
// Расхождение это `Err` с номером первой разошедшейся строки, обеими строками и подсказкой про `UPDATE_GOLDEN=1`. Нет файла:
// тоже `Err` с подсказкой, ничего не создавая. При `update = true` файл записывается (каталоги создаются) и всё проходит.
// Имя образца только из букв, цифр, `_`, `-` и `.`, иначе отказ, и записи за пределами каталога нет.

use std::path::{Path, PathBuf};

fn main() {}

fn golden_path(dir: &Path, name: &str) -> PathBuf {
    dir.join("golden").join(format!("{name}.txt"))
}

pub fn check_golden(dir: &Path, name: &str, actual: &str, update: bool) -> Result<(), String> {
    let _ = (dir, name, actual, update, golden_path(dir, name));
    todo!()
}

pub fn assert_golden(dir: &Path, name: &str, actual: &str) {
    let update = std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1");
    if let Err(message) = check_golden(dir, name, actual, update) {
        panic!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> TestDir {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("svtest7_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(path.join("golden")).unwrap();
            TestDir(path)
        }

        fn seed(&self, name: &str, content: &str) {
            std::fs::write(self.0.join("golden").join(format!("{name}.txt")), content).unwrap();
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const EVENT: &str = "{\n  \"id\": \"e1\",\n  \"kind\": \"post.created\"\n}";

    #[test]
    fn equal_text_passes() {
        let dir = TestDir::new();
        dir.seed("post_created", &format!("{EVENT}\n"));
        assert_eq!(check_golden(&dir.0, "post_created", EVENT, false), Ok(()), "совпадающий вывод проходит");
    }

    #[test]
    fn line_endings_and_the_final_newline_are_not_differences() {
        let dir = TestDir::new();
        dir.seed("crlf", &EVENT.replace('\n', "\r\n"));
        assert_eq!(check_golden(&dir.0, "crlf", EVENT, false), Ok(()), "файл с CRLF равен тексту с LF");
        dir.seed("nl", EVENT);
        assert_eq!(check_golden(&dir.0, "nl", &format!("{EVENT}\n\n"), false), Ok(()), "пустые строки в конце не в счёт");
    }

    #[test]
    fn a_difference_names_the_line_and_both_texts() {
        let dir = TestDir::new();
        dir.seed("e", &format!("{EVENT}\n"));
        let changed = EVENT.replace("post.created", "post.made");
        let message = check_golden(&dir.0, "e", &changed, false).unwrap_err();
        assert!(message.contains("строке 3"), "в сообщении номер первой разошедшейся строки (третья): {message}");
        assert!(message.contains("post.created") && message.contains("post.made"), "и обе строки, ожидаемая и полученная: {message}");
        assert!(message.contains("UPDATE_GOLDEN=1"), "и как обновить образец, если изменение задумано: {message}");
    }

    #[test]
    fn an_extra_or_missing_line_is_a_difference_too() {
        let dir = TestDir::new();
        dir.seed("e", "a\nb\n");
        let longer = check_golden(&dir.0, "e", "a\nb\nc", false).unwrap_err();
        assert!(longer.contains("строке 3") && longer.contains('c'), "лишняя строка найдена: {longer}");
        let shorter = check_golden(&dir.0, "e", "a", false).unwrap_err();
        assert!(shorter.contains("строке 2"), "потерянная строка найдена: {shorter}");
    }

    #[test]
    fn a_missing_sample_is_an_error_that_creates_nothing() {
        let dir = TestDir::new();
        let message = check_golden(&dir.0, "never", "x", false).unwrap_err();
        assert!(message.contains("UPDATE_GOLDEN=1") && message.contains("never"), "подсказка и имя образца: {message}");
        assert!(!dir.0.join("golden").join("never.txt").exists(), "проверка ничего не пишет: иначе первый прогон сам себя одобрит");
    }

    #[test]
    fn update_writes_the_sample_and_the_next_check_passes() {
        let dir = TestDir::new();
        assert_eq!(check_golden(&dir.0.join("fresh"), "new", "line1\nline2\n\n", true), Ok(()), "update создаёт и каталог, и файл");
        let stored = std::fs::read_to_string(dir.0.join("fresh").join("golden").join("new.txt")).unwrap();
        assert_eq!(stored, "line1\nline2\n", "записано ровно с одним переводом строки в конце");
        assert_eq!(check_golden(&dir.0.join("fresh"), "new", "line1\nline2", false), Ok(()), "после обновления обычная проверка проходит");
        dir.seed("old", "was\n");
        assert_eq!(check_golden(&dir.0, "old", "now", true), Ok(()), "update перезаписывает расхождение");
        assert_eq!(check_golden(&dir.0, "old", "now", false), Ok(()), "и новый образец действует");
    }

    #[test]
    fn sample_names_cannot_escape_the_directory() {
        let dir = TestDir::new();
        for name in ["../evil", "a/b", "", ".hidden", "x y"] {
            assert!(check_golden(&dir.0, name, "x", true).is_err(), "имя {name:?} недопустимо");
        }
        assert!(!dir.0.join("evil.txt").exists() && !dir.0.parent().unwrap().join("evil.txt").exists(), "за пределы каталога ничего не записано");
    }
}
