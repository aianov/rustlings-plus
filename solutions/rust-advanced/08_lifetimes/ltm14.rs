pub struct Article {
    body: String,
    title_len: usize,
}

impl Article {
    pub fn new(body: String) -> Article {
        let title_len = body.lines().next().map_or(0, str::len);
        Article { body, title_len }
    }

    pub fn title(&self) -> &str {
        &self.body[..self.title_len]
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn text_after_title(&self) -> &str {
        self.body[self.title_len..].trim_start_matches(['\r', '\n'])
    }
}

fn main() {
    let a = Article::new("Hello\nworld".to_string());
    println!("{} / {}", a.title(), a.text_after_title());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("ltm14.rs");

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
    fn the_struct_has_no_lifetime_parameter() {
        assert!(!compiles_with("pub fn t(_a: Article<'static>) {}"), "у `Article` не должно быть параметра времени жизни: структура не ссылается на себя");
    }

    #[test]
    fn title_is_the_first_line() {
        let a = Article::new("Lifetimes\nare about borrows\nnot about time".to_string());
        assert_eq!(a.title(), "Lifetimes");
        assert_eq!(a.text_after_title(), "are about borrows\nnot about time");
        assert!(a.body().starts_with("Lifetimes\n"));
    }

    #[test]
    fn edge_cases() {
        let one = Article::new("Only title".to_string());
        assert_eq!((one.title(), one.text_after_title()), ("Only title", ""));
        let empty = Article::new(String::new());
        assert_eq!((empty.title(), empty.text_after_title()), ("", ""));
        let blank = Article::new("\n\nbody".to_string());
        assert_eq!((blank.title(), blank.text_after_title()), ("", "body"), "пустой заголовок и пропущенные пустые строки");
        let crlf = Article::new("Head\r\nTail".to_string());
        assert_eq!(crlf.title(), "Head");
        assert_eq!(crlf.text_after_title(), "Tail");
    }

    #[test]
    fn the_title_is_not_copied() {
        let a = Article::new("Same memory\nrest".to_string());
        let body = a.body().as_ptr();
        assert_eq!(a.title().as_ptr(), body, "заголовок должен быть куском тела, а не отдельной строкой");
    }

    #[test]
    fn can_be_moved_into_collections() {
        let articles: Vec<Article> = (0..50).map(|i| Article::new(format!("Title {i}\ntext"))).collect();
        let moved = articles;
        assert_eq!(moved[7].title(), "Title 7");
        assert_eq!(moved[49].text_after_title(), "text");
    }
}
