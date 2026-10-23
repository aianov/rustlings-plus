// Библиотека разбора готова, а примеров в документации нет. Пример в `///` это ещё и тест: `cargo test`
// запускает его, и устаревший пример ломает сборку. Допиши документацию так, чтобы примеры проверяли
// поведение, а не просто собирались, и чтобы каждая публичная вещь получила пример подходящего вида.
// Примеры подключают файл как крейт по его имени: `use tst4::parse_port;`.
#[derive(Debug, PartialEq)]
pub enum MaskError {
    NoAt,
    EmptyUser,
}

/// Разбирает порт из строки: от 1 до 65535, только цифры.
pub fn parse_port(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u16>().ok().filter(|&port| port != 0)
}

/// Прячет почту для логов: от имени остаётся первая буква.
pub fn mask_email(email: &str) -> Result<String, MaskError> {
    let (user, domain) = email.split_once('@').ok_or(MaskError::NoAt)?;
    let first = user.chars().next().ok_or(MaskError::EmptyUser)?;
    Ok(format!("{first}***@{domain}"))
}

/// Одноразовый талон: погашается ровно один раз.
pub struct Token(u32);

impl Token {
    pub fn new(id: u32) -> Token {
        Token(id)
    }

    /// Гасит талон и возвращает его номер.
    pub fn redeem(self) -> u32 {
        self.0
    }
}

/// Принимает соединения, пока процесс жив.
pub fn run_forever(addr: &str) -> ! {
    let _ = addr;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

fn main() {
    println!("{:?} {:?}", parse_port("80"), mask_email("a@b"));
    println!("{}", Token::new(1).redeem());
}

#[cfg(test)]
mod grading {
    const SRC: &str = include_str!("tst4.rs");
    struct DocRun {
        ok: bool,
        text: String,
    }

    impl DocRun {
        fn lines(&self) -> Vec<&str> {
            self.text.lines().filter(|l| l.starts_with("test ")).collect()
        }
        fn count(&self, item: &str, kind: &str) -> usize {
            let needle = format!(" - {item} (line ");
            self.lines()
                .into_iter()
                .filter(|l| l.contains(&needle) && l.ends_with("... ok"))
                .filter(|l| match kind {
                    "run" => !l.contains(" - compile"),
                    other => l.contains(&format!(" - {other} ...")),
                })
                .count()
        }
        fn total(&self, word: &str) -> usize {
            self.text
                .lines()
                .filter(|l| l.starts_with("test result:"))
                .map(|l| {
                    l.split(';')
                        .find(|part| part.trim_end().ends_with(word))
                        .and_then(|part| part.split_whitespace().rev().nth(1))
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(0)
                })
                .sum()
        }
    }

    fn run_docs(krate: &str, code: &str) -> DocRun {
        use std::process::{Command, Stdio};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("{krate}_docs_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join(format!("{krate}.rs"));
        std::fs::write(&file, code).expect("не удалось записать файл");
        let built = Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "rlib", "--crate-name", krate, "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc");
        if !built.status.success() {
            let text = String::from_utf8_lossy(&built.stderr).to_string();
            let _ = std::fs::remove_dir_all(&dir);
            return DocRun { ok: false, text };
        }
        let mut child = Command::new("rustdoc")
            .args(["--edition", "2024", "--crate-name", krate, "--crate-type", "lib", "--test"])
            .arg(&file)
            .arg("--extern")
            .arg(format!("{krate}={}", dir.join(format!("lib{krate}.rlib")).display()))
            .arg("-L")
            .arg(&dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("не удалось запустить rustdoc");
        let mut out = child.stdout.take().expect("нет stdout");
        let mut err = child.stderr.take().expect("нет stderr");
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let (mut a, mut b) = (String::new(), String::new());
            let _ = out.read_to_string(&mut a);
            let _ = err.read_to_string(&mut b);
            a + &b
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let status = loop {
            if let Some(status) = child.try_wait().expect("не удалось проверить rustdoc") {
                break Some(status);
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let mut text = reader.join().unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        if status.is_none() {
            text.push_str("\n[rustdoc не уложился в 90 секунд: какой-то пример не заканчивается, ему нужен no_run]\n");
        }
        DocRun { ok: status.is_some_and(|s| s.success()), text }
    }


    fn code() -> &'static str {
        SRC.split("#[cfg(test)]\nmod grading").next().unwrap_or("")
    }

    fn doc_lines_of(item_marker: &str) -> String {
        let code = code();
        let at = code.find(item_marker).unwrap_or_else(|| panic!("нет `{item_marker}`"));
        let before = &code[..at];
        let mut docs: Vec<&str> = Vec::new();
        for line in before.lines().rev() {
            let t = line.trim_start();
            if t.starts_with("///") {
                docs.push(t);
            } else if t.starts_with("#[") || t.is_empty() && docs.is_empty() {
                continue;
            } else {
                break;
            }
        }
        docs.reverse();
        docs.join("\n")
    }

    #[test]
    fn every_public_item_has_an_example_of_the_right_kind() {
        let run = run_docs("tst4", code());
        assert!(run.ok, "документационные тесты не проходят:\n{}", run.text);
        assert!(run.count("parse_port", "run") >= 1, "у parse_port нет обычного примера с проверками");
        assert!(run.count("mask_email", "run") >= 1, "у mask_email нет обычного примера");
        assert!(run.count("Token::redeem", "compile fail") >= 1, "у Token::redeem нужен пример `compile_fail`: второй вызов не должен собираться");
        assert!(run.count("run_forever", "compile") >= 1, "run_forever никогда не возвращается: её пример только собирается (`no_run`)");
        assert_eq!(run.total("ignored"), 0, "примеры с `ignore` не считаются: они ничего не проверяют");
        examples_really_assert();
    }

    #[test]
    fn email_example_uses_hidden_lines_and_question_mark() {
        let docs = doc_lines_of("pub fn mask_email");
        assert!(docs.lines().any(|l| l.trim_start().starts_with("/// # ")), "скрой обвязку примера строками, начинающимися с `# `");
        assert!(docs.contains(")?"), "в примере для Result используй `?`, а не unwrap");
        assert!(!docs.contains("unwrap"), "в документации unwrap учит плохому");
    }

    fn examples_really_assert() {
        let code = code();
        let mutants = [
            ("порт ноль разрешён", "filter(|&port| port != 0)", "filter(|_| true)"),
            ("маска короче", "{first}***@{domain}", "{first}**@{domain}"),
            ("пустое имя не замечается", ".ok_or(MaskError::EmptyUser)?", ".unwrap_or('x')"),
        ];
        let runs: Vec<(&str, DocRun)> = std::thread::scope(|scope| {
            let handles: Vec<_> = mutants
                .iter()
                .map(|&(what, from, to)| {
                    assert!(code.contains(from), "в коде не нашёлся фрагмент `{from}`: не меняй функции, только документацию");
                    let broken = code.replacen(from, to, 1);
                    (what, scope.spawn(move || run_docs("tst4", &broken)))
                })
                .collect();
            handles.into_iter().map(|(what, handle)| (what, handle.join().expect("поток rustdoc упал"))).collect()
        });
        for (what, run) in runs {
            assert!(!run.ok, "примеры не заметили поломку «{what}»: добавь проверку, которая на ней падает");
        }
    }
}
