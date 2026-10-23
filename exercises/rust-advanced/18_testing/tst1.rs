// Модуль `slug` прячет помощника `collapse`, и его хочется проверить отдельным тестом. Тесты написаны,
// но не собираются. Кроме того, обычная сборка (без тестов) ругается на функцию, которая нужна только
// тестам. Расставь код так, чтобы тестами пользовались тесты, а рабочая сборка о них не знала.
// При этом `collapse` остаётся приватным: ради теста наружу ничего не открываем.
mod slug {
    fn collapse(text: &str) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            let ch = if ch.is_alphanumeric() { ch } else { '-' };
            if ch == '-' && out.ends_with('-') {
                continue;
            }
            out.push(ch);
        }
        out
    }

    pub fn make_slug(title: &str) -> String {
        collapse(&title.to_lowercase()).trim_matches('-').to_string()
    }
}

fn sample_titles() -> [(&'static str, &'static str); 3] {
    [("Hello, World!", "hello-world"), ("  Rust   2024 ", "rust-2024"), ("---", "")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_merges_runs_of_separators() {
        assert_eq!(slug::collapse("a  b!!c"), "a-b-c", "подряд идущие разделители должны слипаться в один дефис");
    }

    #[test]
    fn collapse_keeps_edges() {
        assert_eq!(slug::collapse(" ab "), "-ab-", "края collapse не обрезает, это делает make_slug");
    }

    #[test]
    fn slug_of_sample_titles() {
        for (title, want) in sample_titles() {
            assert_eq!(slug::make_slug(title), want, "заголовок {title:?}");
        }
    }
}

fn main() {
    println!("{}", slug::make_slug("Тесты рядом с кодом"));
}

#[cfg(test)]
mod grading {
    use super::slug;

    fn code_only() -> String {
        SRC.split("#[cfg(test)]\nmod grading")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    const SRC: &str = include_str!("tst1.rs");

    fn compile_plain(code: &str, deny_warnings: bool) -> bool {
        let dir = std::env::temp_dir().join(format!("tst1_plain_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("plain.rs");
        std::fs::write(&file, code).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--emit=metadata", "--crate-type", "bin"]);
        if deny_warnings {
            cmd.args(["-D", "warnings"]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }

    #[test]
    fn production_build_is_clean() {
        let code = SRC.split("#[cfg(test)]\nmod grading").next().unwrap_or("");
        assert!(
            compile_plain(code, true),
            "обычная сборка (без тестов) не собирается или ругается: вспомогательный код для тестов не должен попадать в неё"
        );
    }

    #[test]
    fn collapse_stays_private() {
        let code = code_only();
        assert!(code.contains("fn collapse"), "функция collapse должна остаться на месте");
        for bad in ["pub fn collapse", "pub(crate) fn collapse", "pub(super) fn collapse", "pub(in"] {
            assert!(!code.contains(bad), "collapse нельзя делать видимой снаружи (`{bad}`): тест должен жить внутри модуля slug");
        }
    }

    #[test]
    fn tests_live_inside_the_module() {
        let code = code_only();
        let start = code.find("mod slug").expect("нет модуля slug");
        let body = &code[start..];
        let end = body.find("\n}\n").expect("модуль slug не закрыт");
        let inside = &body[..end];
        assert!(inside.contains("#[cfg(test)]"), "тесты приватного кладут в `#[cfg(test)] mod tests` прямо внутри модуля slug");
        assert!(inside.matches("#[test]").count() >= 3, "в модуле slug должны остаться все три теста");
    }

    #[test]
    fn slug_still_works() {
        assert_eq!(slug::make_slug("Привет, мир!"), "привет-мир");
        assert_eq!(slug::make_slug("a--b"), "a-b");
    }
}
