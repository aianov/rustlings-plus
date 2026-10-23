use std::fmt;

#[derive(Debug, PartialEq)]
pub enum PortError {
    Empty,
    Range,
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortError::Empty => f.write_str("порт не указан"),
            PortError::Range => f.write_str("порт должен быть от 1 до 65535"),
        }
    }
}

/// Разбирает порт из строки.
///
/// ```
/// use tst5::{parse_port, PortError};
///
/// assert_eq!(parse_port("80"), Ok(80));
/// assert_eq!(parse_port("x"), Err(PortError::Range));
/// assert_eq!(parse_port(""), Err(PortError::Empty));
/// ```
pub fn parse_port(text: &str) -> Result<u16, PortError> {
    if text.is_empty() {
        return Err(PortError::Empty);
    }
    match text.parse::<u16>() {
        Ok(0) | Err(_) => Err(PortError::Range),
        Ok(port) => Ok(port),
    }
}

/// Человеческое описание результата разбора.
///
/// ```
/// use tst5::describe;
///
/// assert_eq!(describe("80"), "порт 80");
/// assert_eq!(describe("x"), "порт должен быть от 1 до 65535");
/// ```
pub fn describe(text: &str) -> String {
    match parse_port(text) {
        Ok(port) => Port(port).label(),
        Err(error) => error.to_string(),
    }
}

impl PortError {
    /// Текст ошибки для пользователя.
    ///
    /// ```
    /// use tst5::PortError;
    ///
    /// assert_eq!(PortError::Range.to_string(), "порт должен быть от 1 до 65535");
    /// ```
    pub fn user_text(&self) -> String {
        self.to_string()
    }
}

/// Порт, который уже прошёл проверку.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port(pub u16);

impl Port {
    /// Подпись для интерфейса. Порт копируемый, поэтому после вызова он остаётся доступным:
    ///
    /// ```
    /// use tst5::Port;
    ///
    /// let port = Port(80);
    /// let moved = port;
    /// assert_eq!(port.label(), "порт 80");
    /// assert_eq!(moved.label(), "порт 80");
    /// ```
    pub fn label(self) -> String {
        format!("порт {}", self.0)
    }

    /// Число внутри.
    ///
    /// ```
    /// use tst5::Port;
    ///
    /// assert_eq!(Port(8080).get(), 8080);
    /// ```
    pub fn get(self) -> u16 {
        self.0
    }
}

fn main() {
    println!("{}", describe("8080"));
}

#[cfg(test)]
mod grading {
    const SRC: &str = include_str!("tst5.rs");
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

    #[test]
    fn docs_pass_again_and_none_were_dropped() {
        let run = run_docs("tst5", code());
        assert!(run.ok, "документационные тесты не проходят:\n{}", run.text);
        assert_eq!(run.total("ignored"), 0, "примеры с `ignore` не считаются: они ничего не проверяют");
        assert!(run.total("passed") >= 5, "примеров должно остаться не меньше пяти (было пять), удалять их нельзя");
        assert_eq!(run.count("Port::label", "compile fail"), 0, "пример про move устарел: Port теперь Copy, показывай копирование, а не ошибку");
        assert!(!run.lines().iter().any(|l| l.contains(" - compile")), "примеры должны запускаться, а не только собираться (`no_run`)");
        examples_really_assert();
    }

    fn examples_really_assert() {
        let code = code();
        let mutants = [
            ("текст ошибки диапазона другой", "порт должен быть от 1 до 65535", "порт неверный"),
            ("пустая строка даёт Range", "return Err(PortError::Empty);", "return Err(PortError::Range);"),
            ("подпись другая", "format!(\"порт {}\", self.0)", "format!(\"port {}\", self.0)"),
            ("get возвращает не то", "pub fn get(self) -> u16 {\n        self.0", "pub fn get(self) -> u16 {\n        self.0 + 1"),
        ];
        let runs: Vec<(&str, DocRun)> = std::thread::scope(|scope| {
            let handles: Vec<_> = mutants
                .iter()
                .map(|&(what, from, to)| {
                    assert!(code.contains(from), "в коде не нашёлся фрагмент `{from}`: не меняй функции, только документацию");
                    let broken = code.replacen(from, to, 1);
                    (what, scope.spawn(move || run_docs("tst5", &broken)))
                })
                .collect();
            handles.into_iter().map(|(what, handle)| (what, handle.join().expect("поток rustdoc упал"))).collect()
        });
        for (what, run) in runs {
            assert!(!run.ok, "примеры не заметили поломку «{what}»: проверяй результат через assert_eq!");
        }
    }
}
