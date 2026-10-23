pub struct Highlighter<'t, 'q> {
    text: &'t str,
    query: &'q str,
}

impl<'t, 'q> Highlighter<'t, 'q> {
    pub fn new(text: &'t str, query: &'q str) -> Highlighter<'t, 'q> {
        Highlighter { text, query }
    }

    pub fn spans(&self) -> Vec<&'t str> {
        if self.query.is_empty() {
            return Vec::new();
        }
        self.text.matches(self.query).collect()
    }

    pub fn query(&self) -> &'q str {
        self.query
    }
}

pub fn prefer<'a, 'b>(primary: &'a str, fallback: &'b str) -> &'b str
where
    'a: 'b,
{
    if primary.is_empty() { fallback } else { primary }
}

fn main() {
    let h = Highlighter::new("the cat and the hat", "the");
    println!("{:?} {}", h.spans(), prefer("", "zzz"));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm7.rs");

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
    fn finds_every_occurrence() {
        let h = Highlighter::new("the cat and the hat", "the");
        assert_eq!(h.spans(), vec!["the", "the"]);
        assert_eq!(h.query(), "the");
        assert!(Highlighter::new("abc", "").spans().is_empty(), "пустой запрос ничего не ищет");
        assert!(Highlighter::new("abc", "x").spans().is_empty());
    }

    #[test]
    fn prefer_takes_the_primary_unless_empty() {
        assert_eq!(prefer("config", "request"), "config");
        assert_eq!(prefer("", "request"), "request");
    }

    #[test]
    fn spans_outlive_the_query() {
        let article = String::from("rust and more rust");
        let spans = {
            let typed = String::from("rust");
            let h = Highlighter::new(&article, &typed);
            h.spans()
        };
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn the_query_may_outlive_the_text_too() {
        assert!(
            compiles_with("pub fn t(text: &str) -> usize { let q = String::from(\"a\"); let h = Highlighter::new(text, &q); let spans = h.spans(); spans.len() }"),
            "вместе с двумя разными временами жизни структура должна собираться при любом сочетании"
        );
        assert!(
            compiles_with("pub fn t<'a>(config: &'a str) -> usize { let request = String::from(\"x\"); let v = prefer(config, &request); v.len() }"),
            "prefer должна принимать долгий основной и короткий запасной"
        );
        assert!(
            !compiles_with("pub fn t<'a>(request: &'a str) -> &'static str { prefer(\"x\", request) }"),
            "результат prefer не может жить дольше, чем запасной вариант"
        );
    }
}
