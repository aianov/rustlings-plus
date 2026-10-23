// Разбор строки на токены: `tokens(text, separators)` режет текст по любому из символов-разделителей и
// выкидывает пустые куски. Итератор отдаёт куски `text`, а набор разделителей копирует себе в замыкание
// (он нужен только внутри). `parse_line` вызывает `tokens` с разделителями, собранными на лету.
// И вот она не собирается: «`seps` does not live long enough» (E0597). В edition 2024 тип
// `impl Trait` в результате автоматически захватывает все времена жизни входов, включая время жизни ссылки
// `separators`, хотя итератор её не держит. Результат оказался привязан к строке, которая умирает на выходе.
// TODO: Скажи компилятору точно, какие времена жизни входят в тип результата (`use<..>`), и функция соберётся.

pub fn tokens<'a>(text: &'a str, separators: &str) -> impl Iterator<Item = &'a str> {
    let seps = separators.to_string();
    text.split(move |c: char| seps.contains(c)).filter(|piece| !piece.is_empty())
}

pub fn parse_line(text: &str) -> impl Iterator<Item = &str> {
    let seps = String::from(",; ");
    tokens(text, &seps)
}

fn main() {
    println!("{:?}", parse_line("a, b;c").collect::<Vec<_>>());
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm17.rs");

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
    fn splits_on_any_separator() {
        assert_eq!(tokens("a,b;;c d", ",; ").collect::<Vec<_>>(), vec!["a", "b", "c", "d"]);
        assert_eq!(tokens("", ",").count(), 0);
        assert_eq!(tokens(",,,", ",").count(), 0);
        assert_eq!(tokens("no separators", "").collect::<Vec<_>>(), vec!["no separators"]);
    }

    #[test]
    fn parse_line_uses_commas_semicolons_and_spaces() {
        assert_eq!(parse_line("x, y;z").collect::<Vec<_>>(), vec!["x", "y", "z"]);
    }

    #[test]
    fn tokens_are_pieces_of_the_text() {
        let text = String::from("ab,cd");
        let start = text.as_ptr() as usize;
        assert!(tokens(&text, ",").all(|t| (start..start + text.len()).contains(&(t.as_ptr() as usize))), "токены режутся из текста без копий");
    }

    #[test]
    fn separators_may_die_before_the_iterator_is_used() {
        assert!(
            compiles_with("pub fn t(text: &str) -> usize { let it = { let seps = String::from(\",\"); tokens(text, &seps) }; it.count() }"),
            "итератор не держит строку разделителей: она может исчезнуть раньше, чем итератор будет использован"
        );
    }
}
