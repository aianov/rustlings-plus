// Модуль приветствий для профилей. Три функции не собираются, и компилятор жалуется по-разному: где-то
// «missing lifetime specifier», где-то «lifetime may not live long enough». Правило пропуска (elision)
// угадывает время жизни результата только в простых случаях, а в остальных просит тебя сказать явно, у кого
// результат берёт взаймы. Четвёртая, `first_word`, уже в порядке: посмотри, почему ей ничего не нужно.
// TODO: Допиши времена жизни так, чтобы все функции собрались, а результат оставался ссылкой, не `String`.

pub struct Profile {
    nick: Option<String>,
    email: String,
}

impl Profile {
    pub fn new(nick: Option<&str>, email: &str) -> Profile {
        Profile { nick: nick.map(str::to_string), email: email.to_string() }
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn display_name(&self, fallback: &str) -> &str {
        self.nick.as_deref().unwrap_or(fallback)
    }
}

pub fn first_word(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

pub fn greeting() -> &str {
    "hello"
}

pub fn pick(preferred: &str, spare: &str, use_spare: bool) -> &str {
    if use_spare { spare } else { preferred }
}

fn main() {
    let profile = Profile::new(None, "ann@example.org");
    println!("{} {} {}", profile.display_name("guest"), first_word("hi there"), greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm1.rs");

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
    fn display_name_prefers_the_nick() {
        let ann = Profile::new(Some("ann"), "ann@example.org");
        assert_eq!(ann.display_name("guest"), "ann");
        let nobody = Profile::new(None, "x@example.org");
        assert_eq!(nobody.display_name("guest"), "guest", "без ника показываем запасное имя");
        assert_eq!(nobody.email(), "x@example.org");
    }

    #[test]
    fn small_helpers_work() {
        assert_eq!(first_word("  hello big world"), "hello");
        assert_eq!(first_word("   "), "");
        assert_eq!(greeting(), "hello");
        assert_eq!(pick("a", "b", false), "a");
        assert_eq!(pick("a", "b", true), "b");
    }

    #[test]
    fn results_stay_references_with_the_right_lifetimes() {
        assert!(compiles_with("pub fn t1() -> &'static str { greeting() }"), "greeting ничего не берёт взаймы: результат должен жить всю программу ('static)");
        assert!(compiles_with("pub fn t2<'a>(a: &'a str, b: &'a str) -> &'a str { pick(a, b, true) }"), "pick возвращает ссылку, которая живёт не дольше обоих входов");
        assert!(compiles_with("pub fn t3<'a>(p: &'a Profile, f: &'a str) -> &'a str { p.display_name(f) }"), "display_name возвращает ссылку на профиль или на запасное имя, и обе должны быть учтены");
    }
}
