use std::io::{self, Write};

pub struct Journal<W: Write> {
    out: W,
    lines: usize,
}

impl<W: Write> Journal<W> {
    pub fn new(out: W) -> Journal<W> {
        Journal { out, lines: 0 }
    }

    pub fn lines(&self) -> usize {
        self.lines
    }

    pub fn append(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.out, "{line}")?;
        self.lines += 1;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[derive(Default)]
#[must_use = "Batch::with возвращает новую пачку, старая не меняется"]
pub struct Batch {
    items: Vec<u32>,
}

impl Batch {
    pub fn with(mut self, item: u32) -> Batch {
        self.items.push(item);
        self
    }

    pub fn total(&self) -> u32 {
        self.items.iter().sum()
    }
}

fn main() {
    let mut journal = Journal::new(Vec::new());
    let _ = journal.append("started");
    println!("{}", journal.lines());
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("erp12.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    struct FailAfter {
        left: usize,
        data: Vec<u8>,
    }

    impl Write for FailAfter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.left == 0 {
                return Err(io::Error::other("disk is full"));
            }
            let n = buf.len().min(self.left);
            self.left -= n;
            self.data.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn good_writes_are_counted() {
        let mut journal = Journal::new(Vec::new());
        journal.append("one").unwrap();
        journal.append("two").unwrap();
        assert_eq!(journal.lines(), 2);
    }

    #[test]
    fn a_failed_write_is_reported_and_not_counted() {
        let mut journal = Journal::new(FailAfter { left: 8, data: Vec::new() });
        journal.append("first").unwrap();
        let err = journal.append("second line").expect_err("диск кончился, это должно быть видно");
        assert_eq!(err.to_string(), "disk is full");
        assert_eq!(journal.lines(), 1, "вторая строка не записана, значит не посчитана");
    }

    #[test]
    fn finish_reports_a_failed_flush() {
        let journal = Journal::new(FailAfter { left: 100, data: Vec::new() });
        let err = journal.finish().expect_err("сброс буфера не удался, и об этом должны узнать");
        assert_eq!(err.to_string(), "flush failed");
    }

    #[test]
    fn batch_accumulates() {
        let batch = Batch::default().with(2).with(5);
        assert_eq!(batch.total(), 7);
    }

    #[test]
    fn dropping_a_batch_result_is_a_warning() {
        let forgot = "pub fn forgot(b: Batch) { b.with(1); }";
        assert!(
            !compiles_with_lints(forgot, &["unused_must_use"]),
            "забытый результат Batch::with должен быть замечен компилятором (атрибут must_use на типе)"
        );
        let used = "pub fn used(b: Batch) -> Batch { b.with(1) }";
        assert!(compiles_with_lints(used, &["unused_must_use"]), "использованный результат предупреждения не даёт");
    }
}
